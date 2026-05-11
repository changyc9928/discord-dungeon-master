use std::collections::VecDeque;

use async_trait::async_trait;
use rig::{completion::Completion, message::Message, tool::ToolDyn};
use tracing::info;

use crate::{
    llm::{
        Llm,
        core::common::{Cache, LlmCore, ToolFactory},
        error::LlmError,
    },
    tool::types::{
        AbilitiesToolCall, CombatToolCall, IdentityToolCall, InventoryToolCall, MetaToolCall,
        NewDialogueToolCall, NotesToolCall, ProgressionToolCall, SkillsToolCall, SpellToolCall,
        TraitsToolCall,
    },
};

#[async_trait]
pub trait LlmProvider: Send + Sync {
    type Agent: Completion<Self::CompletionModel> + Send + Sync;
    type CompletionModel: rig::completion::CompletionModel + Send + Sync;

    fn core(&self) -> &LlmCore;
    fn core_mut(&mut self) -> &mut LlmCore;
    fn build_agent(
        &self,
        prompt: &str,
        tools: Vec<Box<dyn ToolDyn>>,
    ) -> Result<Self::Agent, LlmError>;

    async fn handle_llm_interaction(
        &mut self,
        discord_id: &str,
        prompt: &str,
        message: &str,
        tools: Vec<ToolFactory>,
    ) -> Result<String, LlmError> {
        let mut memory = self
            .core()
            .cached_context
            .get(discord_id)
            .cloned()
            .unwrap_or(Cache {
                history: vec![],
                prompt: prompt.to_owned(),
                tools,
            });

        let agent = self.build_agent(
            &memory.prompt,
            memory.tools.iter().map(|factory| factory()).collect(),
        )?;

        let mut texts = Vec::new();
        let mut tool_calls = VecDeque::new();
        let mut should_clear_cache = false;

        let response = self
            .core()
            .completion_with_retry(|| async {
                agent
                    .completion(message, memory.history.clone())
                    .await?
                    .send()
                    .await
            })
            .await?;

        memory.history.push(message.into());
        memory.history.push(response.choice.clone().into());

        self.core()
            .collect_response(&response, &mut texts, &mut tool_calls);

        while let Some(tool_call) = tool_calls.pop_front() {
            if tool_call.function.name == "remove_cache" {
                should_clear_cache = true;
                continue;
            }

            let tool_result_text = match serde_json::to_value(tool_call.function.clone()) {
                Ok(payload) => match self.core().tool_service.dispatch(payload).await {
                    Ok(result) => serde_json::to_string(&result)?,
                    Err(err) => serde_json::to_string(&err.to_string())?,
                },
                Err(err) => serde_json::to_string(&err.to_string())?,
            };

            let message = Message::tool_result_with_call_id(
                tool_call.id,
                tool_call.call_id,
                tool_result_text,
            );

            let followup = self
                .core()
                .completion_with_retry(|| async {
                    agent
                        .completion(message.clone(), memory.history.clone())
                        .await?
                        .send()
                        .await
                })
                .await?;

            memory.history.push(message);
            memory.history.push(followup.choice.clone().into());

            self.core()
                .collect_response(&followup, &mut texts, &mut tool_calls);
        }

        if should_clear_cache {
            self.core_mut().cached_context.remove(discord_id);
        } else {
            self.core_mut()
                .cached_context
                .insert(discord_id.to_owned(), memory);
        }

        Ok(texts
            .into_iter()
            .last()
            .unwrap_or_else(|| "<模型没有给予任何回复>".to_owned()))
    }

    async fn start_character_flow(
        &mut self,
        discord_user_id: &str,
        prompt_file: &str,
        tool_factory: ToolFactory,
    ) -> Result<String, LlmError> {
        let prompt = self.core().load_prompt(prompt_file)?;
        let message = self.core().user_prompt(discord_user_id);
        self.handle_llm_interaction(
            discord_user_id,
            &prompt,
            &message,
            LlmCore::character_flow_tools(tool_factory),
        )
        .await
    }

    async fn request_to_llm_impl(
        &mut self,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        let prompt = self.core().load_prompt("main.txt")?;
        let message = self
            .core()
            .channel_prompt(discord_user_id, discord_channel_message);
        let summary = self.core().story_service.get_latest_story().await?;
        let dialogues = self.core().story_service.get_latest_dialogues().await?;
        let mut dialogues = dialogues
            .iter()
            .map(|dialogue| {
                format!(
                    "[{}{}]：{}",
                    dialogue.author_name,
                    if dialogue.author_character.is_empty() {
                        "".to_owned()
                    } else {
                        format!("（{}）", dialogue.author_character)
                    },
                    dialogue.dialogue
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        if dialogues.is_empty() {
            dialogues = "<目前无任何对话记录>".to_owned();
        }

        let prompt = self
            .core()
            .dialogue_summary_prompt(&prompt, &summary, &dialogues);
        let agent = self.build_agent(&prompt, self.core().character_tools())?;

        let reply = self
            .core()
            .completion_with_retry(|| async {
                agent
                    .completion(message.clone(), Vec::<Message>::new())
                    .await?
                    .send()
                    .await
            })
            .await?;

        let mut texts = vec![];
        let mut tool_calls = VecDeque::new();
        self.core()
            .collect_response(&reply, &mut texts, &mut tool_calls);

        Ok(texts.join("\n"))
    }

    async fn store_new_dialogue_impl(
        &mut self,
        message: &str,
        author_id: &str,
        author_name: &str,
    ) -> Result<(), LlmError> {
        let prompt = self.core().load_prompt("new_dialogue.txt")?;
        let prompt = format!("{prompt}\n\nDM的discord ID为{}", self.core().dm_discord_id);
        let agent = self.build_agent(
            &prompt,
            vec![Box::new(NewDialogueToolCall {
                character_sheet_service: self.core().character_sheet_service.clone(),
                story_service: self.core().story_service.clone(),
                dialogue: message.to_owned(),
                author_name: author_name.to_owned(),
            })],
        )?;

        let reply = self
            .core()
            .completion_with_retry(|| async {
                agent
                    .completion(
                        format!(
                            "用户Discord ID {}; 用户名 {}: {}",
                            author_id, author_name, message
                        ),
                        Vec::<Message>::new(),
                    )
                    .await?
                    .send()
                    .await
            })
            .await?;

        let mut texts = vec![];
        let mut tool_calls = VecDeque::new();
        self.core()
            .collect_response(&reply, &mut texts, &mut tool_calls);
        let reply = texts.join("\n");

        info!(reply = %reply, author_id = %author_id, author_name = %author_name, "Stored new dialogue through LLM tool");
        Ok(())
    }

    async fn new_summary_impl(&mut self) -> Result<(), LlmError> {
        let dialogues = self.core().story_service.get_latest_dialogues().await?;
        if dialogues.len() < self.core().compile_trigger {
            return Ok(());
        }

        let prompt = self.core().load_prompt("new_summary.txt")?;
        let story = self.core().story_service.get_latest_story().await?;
        let dialogues = dialogues
            .iter()
            .map(|dialogue| {
                format!(
                    "[玩家{}{}]：{}",
                    dialogue.author_name,
                    if dialogue.author_character.is_empty() {
                        "".to_owned()
                    } else {
                        format!("（角色名：{}）", dialogue.author_character)
                    },
                    dialogue.dialogue
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let message = self.core().new_summary_prompt(&story, &dialogues);
        let agent = self.build_agent(&prompt, vec![])?;

        let reply = self
            .core()
            .completion_with_retry(|| async {
                agent
                    .completion(message.clone(), Vec::<Message>::new())
                    .await?
                    .send()
                    .await
            })
            .await?;

        let mut texts = vec![];
        let mut tool_calls = VecDeque::new();
        self.core()
            .collect_response(&reply, &mut texts, &mut tool_calls);
        let res = texts.join("\n");

        self.core().story_service.insert_new_story(&res).await?;
        self.core().story_service.clear_dialogue_table().await?;

        Ok(())
    }
}

macro_rules! impl_llm_for_provider {
    ($($fn_name:ident => $tool:ident),+ $(,)?) => {
        #[async_trait]
        impl<T> Llm for T
        where
            T: LlmProvider,
        {
            async fn request_to_llm(
                &mut self,
                discord_user_id: &str,
                discord_channel_message: &str,
            ) -> Result<String, LlmError> {
                self.request_to_llm_impl(discord_user_id, discord_channel_message)
                    .await
            }

            async fn conversation_continue(
                &mut self,
                discord_user_id: &str,
                discord_channel_message: &str,
            ) -> Result<String, LlmError> {
                if !self.core().cached_context.contains_key(discord_user_id) {
                    return Ok("对话上下文不存在，请使用slash command来开启你需要的功能对话".to_owned());
                }
                self.handle_llm_interaction(discord_user_id, "", discord_channel_message, vec![])
                    .await
            }

            $(
                async fn $fn_name(
                    &mut self,
                    discord_user_id: &str,
                ) -> Result<String, LlmError> {
                    let service = self.core().character_sheet_service.clone();

                    self.start_character_flow(
                        discord_user_id,
                        concat!(stringify!($fn_name), ".txt"),
                        LlmCore::tool_factory(move || $tool {
                            character_sheet_service: service.clone(),
                        }),
                    )
                    .await
                }
            )+

            async fn store_new_dialogue(
                &mut self,
                message: &str,
                author_id: &str,
                author_name: &str,
            ) -> Result<(), LlmError> {
                self.store_new_dialogue_impl(message, author_id, author_name)
                    .await
            }

            async fn new_summary(&mut self) -> Result<(), LlmError> {
                self.new_summary_impl().await
            }
        }
    };
}

impl_llm_for_provider! {
    add_character_meta => MetaToolCall,
    add_character_identity => IdentityToolCall,
    add_character_progression => ProgressionToolCall,
    add_character_combat => CombatToolCall,
    add_character_inventory => InventoryToolCall,
    add_character_spells => SpellToolCall,
    add_character_abilities => AbilitiesToolCall,
    add_character_skills => SkillsToolCall,
    add_character_traits => TraitsToolCall,
    add_character_notes => NotesToolCall,
}

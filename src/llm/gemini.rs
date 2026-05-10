use std::{
    collections::{HashMap, VecDeque},
    fs,
    sync::Arc,
};

use async_trait::async_trait;
use rig::{
    client::{CompletionClient, ProviderClient},
    completion::{Completion, CompletionResponse, Prompt},
    message::{AssistantContent, Message, ToolCall},
    providers::gemini,
    tool::ToolDyn,
};
use tracing::info;

use crate::{
    character::service::CharacterSheetService,
    discord_bot::MessageSender,
    llm::{LLM, error::LlmError},
    story::service::StoryService,
    tool::{
        service::ToolService,
        types::{
            AbilitiesToolCall, AddItemToolCall, AddSpellToolCall, CombatToolCall,
            GetCharacterByNameToolCall, GetCharacterToolCall, IdentityToolCall, InventoryToolCall,
            MetaToolCall, NewDialogueToolCall, NotesToolCall, ProgressionToolCall, RemoveCache,
            RemoveItemToolCall, SkillsToolCall, SpellToolCall, TraitsToolCall,
            UpdateCharacterLevelToolCall, UpdateCurrentHpToolCall, UpdateMaxHpToolCall,
            UpdateSpellSlotsToolCall,
        },
    },
};

type ToolFactory = Arc<dyn Fn() -> Box<dyn ToolDyn> + Send + Sync>;

#[derive(Clone)]
pub struct Cache {
    pub history: Vec<Message>,
    pub prompt: String,
    pub tools: Vec<ToolFactory>,
}

pub struct Gemini {
    model: String,
    story_service: Arc<StoryService>,
    character_sheet_service: Arc<CharacterSheetService>,
    tool_service: Arc<ToolService>,
    cached_context: HashMap<String, Cache>,
    dm_discord_id: String,
    folder_path: String,
    compile_trigger: i64,
}

impl Gemini {
    pub fn new(
        model: &str,
        tool_service: Arc<ToolService>,
        story_service: Arc<StoryService>,
        character_sheet_service: Arc<CharacterSheetService>,
        dm_discord_id: String,
        folder_path: String,
        compile_trigger: i64,
    ) -> Result<Self, LlmError> {
        Ok(Self {
            model: model.to_owned(),
            story_service,
            character_sheet_service,
            cached_context: HashMap::new(),
            dm_discord_id,
            folder_path,
            compile_trigger,
            tool_service,
        })
    }

    async fn handle_llm_interaction(
        &mut self,
        discord_id: &str,
        prompt: &str,
        message: &str,
        tools: Vec<ToolFactory>,
    ) -> Result<String, LlmError> {
        let mut memory = self
            .cached_context
            .get(discord_id)
            .cloned()
            .unwrap_or(Cache {
                history: vec![],
                prompt: prompt.to_owned(),
                tools,
            });

        let agent = gemini::Client::from_env()?
            .agent(self.model.clone())
            .preamble(&memory.prompt)
            .tools(memory.tools.iter().map(|f| f()).collect())
            .build();

        let mut texts = Vec::new();
        let mut tool_calls = VecDeque::new();
        let mut should_clear_cache = false;

        // Initial completion
        let response = agent
            .completion(message, memory.history.clone())
            .await?
            .send()
            .await?;

        memory.history.push(message.into());
        memory.history.push(response.choice.clone().into());

        self.collect_response(&response, &mut texts, &mut tool_calls);

        while let Some(tool_call) = tool_calls.pop_front() {
            if tool_call.function.name == "remove_cache" {
                should_clear_cache = true;
                continue;
            }

            let tool_result_text = match serde_json::to_value(tool_call.function.clone()) {
                Ok(payload) => match self.tool_service.dispatch(payload).await {
                    Ok(result) => serde_json::to_string(&result)?,
                    Err(err) => serde_json::to_string(&err.to_string())?,
                },

                Err(err) => serde_json::to_string(&err.to_string())?,
            };

            let message = Message::tool_result(tool_call.id, tool_result_text);

            let followup = agent
                .completion(message.clone(), memory.history.clone())
                .await?
                .send()
                .await?;

            memory.history.push(message);
            memory.history.push(followup.choice.clone().into());

            self.collect_response(&followup, &mut texts, &mut tool_calls);
        }

        if should_clear_cache {
            self.cached_context.remove(discord_id);
        } else {
            self.cached_context.insert(discord_id.to_owned(), memory);
        }

        Ok(texts
            .into_iter()
            .last()
            .unwrap_or_else(|| "<模型没有给予任何回复>".to_owned()))
    }

    fn collect_response<T>(
        &self,
        response: &CompletionResponse<T>,
        texts: &mut Vec<String>,
        tool_calls: &mut VecDeque<ToolCall>,
    ) {
        for item in response.choice.iter() {
            match item {
                AssistantContent::Text(text) => {
                    texts.push(text.text.clone());
                }

                AssistantContent::ToolCall(call) => {
                    tool_calls.push_back(call.clone());
                }

                AssistantContent::Reasoning(reasoning) => {
                    info!("Reasoning: {reasoning:?}");
                }

                AssistantContent::Image(image) => {
                    info!("Image sent: {image:?}");
                }
            }
        }
    }
}

#[async_trait]
impl LLM for Gemini {
    async fn conversation_continue(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        if !self.cached_context.contains_key(discord_user_id) {
            return Ok("对话上下文不存在，请使用slash command来开启你需要的功能对话".to_owned());
        }
        self.handle_llm_interaction(discord_user_id, "", discord_channel_message, vec![])
            .await
    }

    async fn add_character_spells(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/add_character_spells.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(SpellToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_abilities(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt =
            fs::read_to_string(format!("{}/add_character_abilities.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(AbilitiesToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_skills(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/add_character_skills.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(SkillsToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_traits(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/add_character_traits.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(TraitsToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_notes(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/add_character_notes.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(NotesToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_meta(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/add_character_meta.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(MetaToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_identity(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt =
            fs::read_to_string(format!("{}/add_character_identity.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(IdentityToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_progression(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!(
            "{}/add_character_progression.txt",
            self.folder_path
        ))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(ProgressionToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_combat(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/add_character_combat.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(CombatToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn add_character_inventory(
        &mut self,
        ctx: &dyn MessageSender,
        discord_user_id: &str,
        discord_username: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt =
            fs::read_to_string(format!("{}/add_character_inventory.txt", self.folder_path))?;

        let character_sheet_service = self.character_sheet_service.clone();

        let tools: Vec<ToolFactory> = vec![
            Arc::new(move || {
                Box::new(InventoryToolCall {
                    character_sheet_service: character_sheet_service.clone(),
                })
            }),
            Arc::new(move || Box::new(RemoveCache)),
        ];

        let message = format!("你好，我的Discord ID是{discord_user_id}，请问你要什么信息？");

        self.handle_llm_interaction(discord_user_id, &prompt, &message, tools)
            .await
    }

    async fn request_to_llm(
        &mut self,
        ctx: &dyn MessageSender,
        discord_username: &str,
        discord_user_id: &str,
        discord_channel_message: &str,
    ) -> Result<String, LlmError> {
        let _ = (ctx, discord_username);
        let prompt = fs::read_to_string(format!("{}/main.txt", self.folder_path))?;

        let message = format!(
            "Discord channnel里的用户{}发送了消息：{}",
            discord_user_id, discord_channel_message
        );

        let summary = self.story_service.get_latest_story().await?;
        let dialogues = self.story_service.get_latest_dialogues().await?;
        let mut dialogues = dialogues
            .iter()
            .map(|d| {
                format!(
                    "[{}{}]：{}",
                    d.author_name,
                    if d.author_character.is_empty() {
                        "".to_string()
                    } else {
                        format!("（{}）", d.author_character)
                    },
                    d.dialogue
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        if dialogues.is_empty() {
            dialogues = "<目前无任何对话记录>".to_string();
        }

        let prompt = format!(
            "{}

DM的discord ID为{}

【背景信息-剧情总结（用于理解当前局势）】
{summary}

【背景信息-最近对话（按时间顺序）】
{dialogues}

【说明】
- 上述内容仅作为背景信息
- 请基于这些信息进行判断
- 不要重复或总结上述内容",
            prompt, self.dm_discord_id
        );

        let client = gemini::Client::from_env()?
            .agent(self.model.clone())
            .preamble(&prompt)
            .tools(vec![
                Box::new(GetCharacterToolCall {
                    character_sheet_service: self.character_sheet_service.clone(),
                }),
                Box::new(GetCharacterByNameToolCall {
                    character_sheet_service: self.character_sheet_service.clone(),
                }),
                Box::new(AddItemToolCall {
                    character_sheet_service: self.character_sheet_service.clone(),
                }),
                Box::new(RemoveItemToolCall {
                    character_sheet_service: self.character_sheet_service.clone(),
                }),
                Box::new(AddSpellToolCall {
                    character_sheet_service: self.character_sheet_service.clone(),
                }),
                Box::new(UpdateSpellSlotsToolCall {
                    character_sheet_service: self.character_sheet_service.clone(),
                }),
                Box::new(UpdateCurrentHpToolCall {
                    character_sheet_service: self.character_sheet_service.clone(),
                }),
                Box::new(UpdateMaxHpToolCall {
                    character_sheet_service: self.character_sheet_service.clone(),
                }),
                Box::new(UpdateCharacterLevelToolCall {
                    character_sheet_service: self.character_sheet_service.clone(),
                }),
            ])
            .build();

        let reply = client.prompt(message).await?;

        Ok(reply)
    }

    async fn store_new_dialogue(
        &mut self,
        ctx: &dyn MessageSender,
        message: &str,
        author_id: &str,
        author_name: &str,
    ) -> Result<(), LlmError> {
        let _ = ctx;
        let prompt = fs::read_to_string(format!("{}/new_dialogue.txt", self.folder_path))?;

        let prompt = format!(
            "{prompt}

DM的discord ID为{}",
            self.dm_discord_id
        );

        let client = gemini::Client::from_env()?
            .agent(self.model.clone())
            .preamble(&prompt)
            .tool(NewDialogueToolCall {
                character_sheet_service: self.character_sheet_service.clone(),
                story_service: self.story_service.clone(),
                dialogue: message.to_owned(),
                author_name: author_name.to_owned(),
            })
            .build();

        let reply = client
            .prompt(format!(
                "用户Discord ID {}; 用户名 {}: {}",
                author_id, author_name, message
            ))
            .await?;

        info!(reply = %reply, author_id = %author_id, author_name = %author_name, "Stored new dialogue through LLM tool");

        Ok(())
    }

    async fn new_summary(&mut self, ctx: &dyn MessageSender) -> Result<(), LlmError> {
        let _ = ctx;
        let dialogues = self.story_service.get_latest_dialogues().await?;
        if dialogues.len() < self.compile_trigger as usize {
            return Ok(());
        }
        let prompt = fs::read_to_string(format!("{}/new_summary.txt", self.folder_path))?;

        let story = self.story_service.get_latest_story().await?;

        let dialogues = dialogues
            .iter()
            .map(|d| {
                format!(
                    "[玩家{}{}]：{}",
                    d.author_name,
                    if d.author_character.is_empty() {
                        "".to_string()
                    } else {
                        format!("（角色名：{}）", d.author_character)
                    },
                    d.dialogue
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let message = format!(
            "【历史剧情总结】
{story}

【最近对话记录（按时间顺序）】
{dialogues}

【任务】
请基于以上信息，生成一段更新后的完整剧情总结。"
        );

        let client = gemini::Client::from_env()?
            .agent(self.model.clone())
            .preamble(&prompt)
            .build();

        let res = client.prompt(message).await?;

        self.story_service.insert_new_story(&res).await?;

        self.story_service.clear_dialogue_table().await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, sync::Arc, thread::sleep, time::Duration};

    use chrono::Utc;
    use insta::assert_json_snapshot;
    use serenity::async_trait;
    use sqlx::{Pool, Postgres};

    use crate::{
        character::{
            entity::{
                Ability, CharacterSheet,
                abilities_block::{AbilitiesBlock, AbilityScore},
                combat::{Action, Combat, CombatAction, Defenses, SavingThrows, Sense, Speed},
                identity::{Characteristics, Identity},
                inventory::{Inventory, Item},
                meta::Meta,
                notes::Notes,
                progression::{ProficianciesTrainings, Progression},
                skills::{SkillStatus, Skills},
                spells::{Spell, SpellSlot, Spells},
                traits::{FeatureTraits, LockedFeatureTraits, Traits},
            },
            repository::CharacterSheetRepository,
            service::CharacterSheetService,
        },
        config::{AiDmConfig, ServiceConfig},
        discord_bot::MessageSender,
        llm::{LLM, gemini::Gemini},
        pg_pool::{TestPgPool, TestPgPoolConfig},
        story::{
            entity::{DialogueEntity, StoryEntity},
            repository::{DialogueRepository, StoryRepository},
            service::StoryService,
        },
        tool::service::ToolService,
    };

    struct MockMessageSender;

    #[async_trait]
    impl MessageSender for MockMessageSender {
        async fn send(&self, msg: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            println!("Mock send message: {}", msg);
            Ok(())
        }
    }

    async fn service_setup() -> Result<
        (Gemini, Pool<Postgres>, String, Arc<CharacterSheetService>),
        Box<dyn std::error::Error>,
    > {
        let service_config: ServiceConfig<AiDmConfig> =
            ServiceConfig::load("./config/config.yaml")?;
        let db_config = service_config
            .database
            .as_ref()
            .ok_or_else(|| crate::error::Error::MissingConfig("database"))?;
        let timestamp = Utc::now().timestamp();
        let db_name = format!("test_db_{}_{}", timestamp, rand::random::<u16>());
        let pg_pool = TestPgPool::init(TestPgPoolConfig {
            migrations: "./migrations".into(),
            db_name: db_name.clone(),
            host: db_config.host.clone(),
            port: db_config.port,
            default_database: "postgres".to_owned(),
            username: db_config.username.clone(),
            password: db_config.password.clone(),
        })
        .await;
        let pool = pg_pool.resource().await;

        let character_sheet_repository =
            Arc::new(CharacterSheetRepository::from_pool(pool.clone()));
        let story_repository = Arc::new(StoryRepository::from_pool(pool.clone()));
        let dialogue_repository = Arc::new(DialogueRepository::from_pool(pool.clone()));

        let character_sheet_service = Arc::new(CharacterSheetService {
            repo: character_sheet_repository,
        });
        let story_service = Arc::new(StoryService {
            repository: story_repository,
            dialogue_repository,
            compile_trigger: 10,
        });
        let tool_service = Arc::new(ToolService::new(
            character_sheet_service.clone(),
            story_service.clone(),
        ));

        Ok((
            Gemini {
                model: "gemini-3.1-flash-lite-preview".to_owned(),
                story_service,
                character_sheet_service: character_sheet_service.clone(),
                cached_context: HashMap::new(),
                dm_discord_id: "1483098634601107476".to_owned(),
                folder_path: "./prompts".to_string(),
                compile_trigger: 4,
                tool_service,
            },
            pool,
            db_name,
            character_sheet_service,
        ))
    }

    fn test_character() -> CharacterSheet {
        CharacterSheet {
            meta: Meta {
                discord_id: "400114655500042240".to_owned(),
                location: "Tavern".to_owned(),
                story_summary: Some("Just started to join the game".to_owned()),
                extra_creatures: vec![],
                dead: false,
                performing_action: Some("Drinking".to_owned()),
                action_end_time: Some("Harptos 24 Mar 1555 12:35PM".to_owned()),
            },
            identity: Identity {
                character_name: "真珠".to_owned(),
                species: "Aasimar".to_owned(),
                sub_species: None,
                class: "Sorcerer".to_owned(),
                sub_class: Some("Draconic Bloodline - Gold".to_owned()),
                characteristics: Characteristics {
                    background: "Noble".to_owned(),
                    background_feature: "Position of Privilege".to_owned(),
                    background_feature_description: "Thanks to your noble birth, \
                        people are inclined to think the best of you. \
                        You are welcome in high society, \
                        and people assume you have the right \
                        to be wherever you are. \
                        The common folk make every effort to accommodate you and \
                        avoid your displeasure, and other people of high birth \
                        treat you as a member of the same social sphere. \
                        You can secure an audience with a local noble \
                        if you need to."
                        .to_owned(),
                    alignment: "Lawful neutral".to_owned(),
                    gender: "Female".to_owned(),
                    eyes: "Blue".to_owned(),
                    size: "Medium".to_owned(),
                    height: "5'7″".to_owned(),
                    faith: "Amber Lord".to_owned(),
                    hair: "Blonde".to_owned(),
                    skin: "White".to_owned(),
                    age: 28,
                    weight: "104 lbs.".to_owned(),
                    personality_traits: "My eloquent flattery makes everyone \
                        I talk to feel like the most wonderful and important \
                        person in the world."
                        .to_owned(),
                    ideals: "Responsibility. It is my duty to respect \
                        the authority of those above me, just as those below \
                        me must respect mine. (Lawful)"
                        .to_owned(),
                    bonds: "The common folk must see me as a hero of the people.".to_owned(),
                    flaws: "I too often hear veiled insults and threats \
                        in every word addressed to me, and I'm quick to anger."
                        .to_owned(),
                    appearance_trait: vec![],
                },
            },
            progression: Progression {
                level: 2,
                xp: 330,
                total_hit_dice: "2d6".to_owned(),
                max_hp: 16,
                proficiencies: ProficianciesTrainings {
                    proficiency_bonus: 2,
                    armor: vec![],
                    weapons: vec![
                        "Crossbow".to_owned(),
                        "Light".to_owned(),
                        "Dagger".to_owned(),
                        "Dart".to_owned(),
                        "Quarterstaff".to_owned(),
                        "Sling".to_owned(),
                    ],
                    tools: vec![
                        "Dragonchess Set".to_owned(),
                        "Painter's Supplies".to_owned(),
                    ],
                    languages: vec![
                        "Celestial".to_owned(),
                        "Common".to_owned(),
                        "Draconic".to_owned(),
                    ],
                },
            },
            combat: Combat {
                armor_class: 15,
                initiative: 2,
                hit_points: 16,
                speed: vec![Speed {
                    name: "Walking".to_owned(),
                    value: "30 ft".to_owned(),
                }],
                senses: vec![Sense {
                    name: "Darkvision".to_owned(),
                    value: "60 ft".to_owned(),
                }],
                defenses: Defenses {
                    resistance: vec!["Necrotic".to_owned(), "Radiant".to_owned()],
                    immunities: vec![],
                    vulnerabilities: vec![],
                },
                conditions: vec![],
                exhaustion_level: 0,
                saving_throws: SavingThrows {
                    proficiency: vec![Ability::Constitution, Ability::Charisma],
                    constitution_saving_throws: 0,
                    strength_saving_throws: 0,
                    intelligence_saving_throws: 0,
                    dexterity_saving_throws: 0,
                    wisdom_saving_throws: 0,
                    charisma_saving_throws: 0,
                },
                actions: vec![
                    Action {
                        name: "Attack".to_owned(),
                        used_time: None,
                        max_use_time: None,
                    },
                    Action {
                        name: "Dash".to_owned(),
                        used_time: None,
                        max_use_time: None,
                    },
                    Action {
                        name: "Disengage".to_owned(),
                        used_time: None,
                        max_use_time: None,
                    },
                ],
                combat_actions: vec![
                    CombatAction {
                        name: "Dagger".to_owned(),
                        hit_dc: Some(4),
                        damage: "1d4+2".to_owned(),
                    },
                    CombatAction {
                        name: "Dagger".to_owned(),
                        hit_dc: Some(4),
                        damage: "1d4+2".to_owned(),
                    },
                    CombatAction {
                        name: "Ray of Front".to_owned(),
                        hit_dc: Some(5),
                        damage: "1d8".to_owned(),
                    },
                    CombatAction {
                        name: "Unarmed Strike".to_owned(),
                        hit_dc: Some(1),
                        damage: "0".to_owned(),
                    },
                ],
            },
            abilities_block: AbilitiesBlock {
                strength: AbilityScore {
                    base: 8,
                    modifier: 0,
                },
                dexterity: AbilityScore {
                    base: 14,
                    modifier: 0,
                },
                constitution: AbilityScore {
                    base: 14,
                    modifier: 0,
                },
                intelligence: AbilityScore {
                    base: 10,
                    modifier: 0,
                },
                wisdom: AbilityScore {
                    base: 11,
                    modifier: 0,
                },
                charisma: AbilityScore {
                    base: 17,
                    modifier: 0,
                },
            },
            skills: Skills {
                acrobatics: SkillStatus {
                    prof: false,
                    bonus: 2,
                    modifier: Ability::Dexterity,
                    passive: 0,
                },
                animal_handling: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Wisdom,
                    passive: 0,
                },
                arcana: SkillStatus {
                    prof: true,
                    bonus: 2,
                    modifier: Ability::Intelligence,
                    passive: 0,
                },
                athletics: SkillStatus {
                    prof: false,
                    bonus: -1,
                    modifier: Ability::Strength,
                    passive: 0,
                },
                deception: SkillStatus {
                    prof: false,
                    bonus: 3,
                    modifier: Ability::Charisma,
                    passive: 0,
                },
                history: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Intelligence,
                    passive: 0,
                },
                insight: SkillStatus {
                    prof: true,
                    bonus: 2,
                    modifier: Ability::Wisdom,
                    passive: 0,
                },
                intimidation: SkillStatus {
                    prof: true,
                    bonus: 5,
                    modifier: Ability::Charisma,
                    passive: 0,
                },
                investigation: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Intelligence,
                    passive: 0,
                },
                medicine: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Wisdom,
                    passive: 0,
                },
                nature: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Intelligence,
                    passive: 0,
                },
                perception: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Wisdom,
                    passive: 0,
                },
                performance: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Charisma,
                    passive: 0,
                },
                persuasion: SkillStatus {
                    prof: true,
                    bonus: 0,
                    modifier: Ability::Charisma,
                    passive: 0,
                },
                religion: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Intelligence,
                    passive: 0,
                },
                sleight_of_hand: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Dexterity,
                    passive: 0,
                },
                stealth: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Dexterity,
                    passive: 0,
                },
                survival: SkillStatus {
                    prof: false,
                    bonus: 0,
                    modifier: Ability::Wisdom,
                    passive: 0,
                },
            },
            magic: Spells {
                    spells: vec![
                        Spell {
                            name: "Light".to_owned(),
                            level: 0,
                            cast_time: "1 action".to_owned(),
                            range: "Touch".to_owned(),
                            hit_dc: Some(13),
                            effect: "Creation".to_owned(),
                        },
                        Spell {
                            name: "Message".to_owned(),
                            level: 0,
                            cast_time: "1 action".to_owned(),
                            range: "120 ft.".to_owned(),
                            hit_dc: None,
                            effect: "Communication".to_owned(),
                        },
                        Spell {
                            name: "Minor Illusion".to_owned(),
                            level: 0,
                            cast_time: "1 action".to_owned(),
                            range: "30 ft.".to_owned(),
                            hit_dc: None,
                            effect: "Control".to_owned(),
                        },
                        Spell {
                            name: "Prestidigitation".to_owned(),
                            level: 0,
                            cast_time: "1 action".to_owned(),
                            range: "10 ft.".to_owned(),
                            hit_dc: None,
                            effect: "Utility".to_owned(),
                        },
                        Spell {
                            name: "Ray of Frost".to_owned(),
                            level: 0,
                            cast_time: "1 action".to_owned(),
                            range: "60 ft.".to_owned(),
                            hit_dc: Some(5),
                            effect: "1d8".to_owned(),
                        },
                        Spell {
                            name: "Magic Missile".to_owned(),
                            level: 1,
                            cast_time: "1 action".to_owned(),
                            range: "120 ft.".to_owned(),
                            hit_dc: None,
                            effect: "1d4+1".to_owned(),
                        },
                        Spell {
                            name: "Shield".to_owned(),
                            level: 1,
                            cast_time: "1 reaction".to_owned(),
                            range: "Self".to_owned(),
                            hit_dc: None,
                            effect: "Warding".to_owned(),
                        },
                        Spell {
                            name: "Thunderwave".to_owned(),
                            level: 1,
                            cast_time: "1 action".to_owned(),
                            range: "Self".to_owned(),
                            hit_dc: Some(13),
                            effect: "2d8".to_owned(),
                        },
                    ],
                    spell_slots: vec![SpellSlot {
                        level: 1,
                        slot: 3,
                        used: 0,
                    }],
                    ability_type: Ability::Charisma,
                    ability_modifier: 3,
                    spell_attack: 5,
                    save_dc: 13,
            },
            inventory: Inventory {
                items: vec![
                    Item {
                        name: "Clothes, Fine".to_owned(),
                        weight: 6,
                        quantity: Some(1),
                        cost_gp: 15,
                        equiped: false,
                    },
                    Item {
                        name: "Dagger".to_owned(),
                        weight: 1,
                        quantity: None,
                        cost_gp: 2,
                        equiped: true,
                    },
                ],
            },
            traits: Traits {
                unlocked_features_and_traits: vec![
                                    FeatureTraits {
                                        name: "HIT POINTS".to_owned(),
                                        description: "Hit Dice: 1d6 per Sorcerer level
                Hit Points at Level 1: 6 + your Constitution modifier
                Hit Points per Later Level: 1d6 (or 4) + your Constitution modifier"
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                                    FeatureTraits {
                                        name: "PROFICIENCIES".to_owned(),
                                        description: "Saving Throws: Constitution, Charisma
                Skills (Choose 2): Arcana, Deception, Insight, Intimidation, Persuasion, Religion
                Weapons: Simple Weapons
                Tools: None".to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                                    FeatureTraits {
                                        name: "ARMOR TRAINING".to_owned(),
                                        description: "None".to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                                    FeatureTraits {
                                        name: "STARTING EQUIPMENT".to_owned(),
                                        description: "As a level 1 character, you start with the following equipment, or you can forgo it and spend 50 GP on equipment of your choice.

                Arcane Focus (Crystal)
                Dagger (x2)
                Dungeoneer’s Pack
                Spear
                28 GP".to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                                    FeatureTraits {
                                        name: "SORCERER CLASS FEATURES".to_owned(),
                                        description: "| Level | Proficiency Bonus | Sorcery Points | Features | Cantrips Known | Spells Known | 1st | 2nd | 3rd | 4th | 5th | 6th | 7th | 8th | 9th |
                |-------|-------------------|----------------|----------|----------------|--------------|-----|-----|-----|-----|-----|-----|-----|-----|-----|
                | 1st  | +2 | —  | Spellcasting, Sorcerous Origin | 4 | 2  | 2 | — | — | — | — | — | — | — | — |
                | 2nd  | +2 | 2  | Font of Magic | 4 | 3  | 3 | — | — | — | — | — | — | — | — |
                | 3rd  | +2 | 3  | Metamagic | 4 | 4  | 4 | 2 | — | — | — | — | — | — | — |
                | 4th  | +2 | 4  | Ability Score Improvement | 5 | 5  | 4 | 3 | — | — | — | — | — | — | — |
                | 5th  | +3 | 5  | — | 5 | 6  | 4 | 3 | 2 | — | — | — | — | — | — |
                | 6th  | +3 | 6  | Sorcerous Origin Feature | 5 | 7  | 4 | 3 | 3 | — | — | — | — | — | — |
                | 7th  | +3 | 7  | — | 5 | 8  | 4 | 3 | 3 | 1 | — | — | — | — | — |
                | 8th  | +3 | 8  | Ability Score Improvement | 5 | 9  | 4 | 3 | 3 | 2 | — | — | — | — | — |
                | 9th  | +4 | 9  | — | 5 | 10 | 4 | 3 | 3 | 3 | 1 | — | — | — | — |
                | 10th | +4 | 10 | Metamagic | 6 | 11 | 4 | 3 | 3 | 3 | 2 | — | — | — | — |
                | 11th | +4 | 11 | — | 6 | 12 | 4 | 3 | 3 | 3 | 2 | 1 | — | — | — |
                | 12th | +4 | 12 | Ability Score Improvement | 6 | 12 | 4 | 3 | 3 | 3 | 2 | 1 | — | — | — |
                | 13th | +5 | 13 | — | 6 | 13 | 4 | 3 | 3 | 3 | 2 | 1 | 1 | — | — |
                | 14th | +5 | 14 | Sorcerous Origin Feature | 6 | 13 | 4 | 3 | 3 | 3 | 2 | 1 | 1 | — | — |
                | 15th | +5 | 15 | — | 6 | 14 | 4 | 3 | 3 | 3 | 2 | 1 | 1 | 1 | — |
                | 16th | +5 | 16 | Ability Score Improvement | 6 | 14 | 4 | 3 | 3 | 3 | 2 | 1 | 1 | 1 | — |
                | 17th | +6 | 17 | Metamagic | 6 | 15 | 4 | 3 | 3 | 3 | 2 | 1 | 1 | 1 | 1 |
                | 18th | +6 | 18 | Sorcerous Origin Feature | 6 | 15 | 4 | 3 | 3 | 3 | 3 | 1 | 1 | 1 | 1 |
                | 19th | +6 | 19 | Ability Score Improvement | 6 | 15 | 4 | 3 | 3 | 3 | 3 | 2 | 1 | 1 | 1 |
                | 20th | +6 | 20 | Sorcerous Restoration | 6 | 15 | 4 | 3 | 3 | 3 | 3 | 2 | 2 | 1 | 1 |".to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                                    FeatureTraits {
                                        name: "INNATE SORCERY".to_owned(),
                                        description: "An event in your past left an indelible mark on you, infusing you with a simmering magic. As a Bonus Action, you can unleash that magic for 1 minute, during which you gain the following benefits:

                The spell save DC of your Sorcerer spells increases by 1.
                You have Advantage on the attack rolls of Sorcerer spells you cast.
                You can use this feature twice, and you regain all expended uses of it when you finish a Long Rest.".to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: Some(0),
                                        max_charges: Some(2),
                                    },
                                    FeatureTraits {
                                        name: "SPELLCASTING".to_owned(),
                                        description: "Drawing from your innate magic, you can cast spells. See the Player’s Handbook for rules on spellcasting. The information below details how you use those rules as a Sorcerer.

                Cantrips. You know four cantrips of your choice from the Sorcerer spell list. Rather than choosing, you may start with Light, Prestidigitation, Shocking Grasp, and Sorcerous Burst. Whenever you gain a Sorcerer level, you can replace one of your cantrips from this feature with another Sorcerer cantrip of your choice. When you reach levels 4 and 10 in this class, you learn another Sorcerer cantrip of your choice, as shown in the Cantrips column of the Sorcerer table.
                Spell Slots. The Sorcerer table shows how many spell slots you have to cast your spells of level 1 and higher. To cast one of these spells, you must expend a slot of the spell’s level or higher. You regain all expended spell slots when you finish a Long Rest. Prepared Spells of Level 1+. You prepare the list of spells of level 1 and higher that are available for you to cast with this feature. To start, choose two level 1 spells from the Sorcerer spell list. Rather than choosing, you may start with Burning Hands and Detect Magic. The number of spells on your list also increases as you gain Sorcerer levels, as shown in the Prepared Spells column of the Sorcerer table. Whenever that number increases, choose additional spells from the Sorcerer spell list until the number of spells on your list matches the number on the table. The chosen spells must be of a level for which you have spell slots. For example, if you’re a level 3 Sorcerer, your list of prepared spells can include six Sorcerer spells of level 1 or 2 in any combination. If another Sorcerer feature gives spells that you always have prepared, those spells don’t count against the number of spells on the list you prepare with this Spellcasting feature, but those spells otherwise follow the rules in this feature.
                Changing Your Prepared Spells. Whenever you gain a Sorcerer level, you can replace one spell on your list with another Sorcerer spell for which you have spell slots.
                Spellcasting Ability. Charisma is your spellcasting ability for the spells you cast with your Sorcerer features.
                Spellcasting Focus. You can use an Arcane Focus as a Spellcasting Focus for the spells you cast with your Sorcerer features."
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                                    FeatureTraits {
                                        name: "FONT OF MAGIC".to_owned(),
                                        description: "You can tap into the wellspring of magic within yourself. This wellspring is represented by Sorcery Points, which allow you to create a variety of magical effects. You have 2 Sorcery Points, and you gain more as you reach higher levels, as shown in the Sorcery Points column of the Sorcerer table. You can never have more Sorcery Points than the number shown on the table for your level. You regain all spent Sorcery Points when you finish a Long Rest. You can use your Sorcery Points to fuel the options below, along with other features, such as Metamagic, that use those points.

                Converting Spell Slots to Sorcery Points. You can expend a spell slot to gain a number of Sorcery Points equal to the slot’s level (no action required).
                Creating Spell Slots. As a Bonus Action, you can transform unexpended Sorcery Points into one spell slot. The Creating Spell Slots table shows the cost of creating a spell slot of a given level, and it lists the minimum Sorcerer level you must be to create a slot. You can create a spell slot no higher in level than 5. Any spell slot you create with this feature vanishes when you finish a Long Rest.

                CREATING SPELL SLOTS:
                | Spell Slot Level | Sorcery Point Cost | Min. Sorcerer Level |
                |------------------|--------------------|---------------------|
                | 1 | 2 | 2 |
                | 2 | 3 | 3 |
                | 3 | 5 | 5 |
                | 4 | 6 | 7 |
                | 5 | 7 | 9 |"
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: Some(0),
                                        max_charges: Some(2),
                                    },
                                    FeatureTraits {
                                        name: "METAMAGIC".to_owned(),
                                        description: "You gain two Metamagic options of your choice from the Metamagic Options. You use the chosen options to temporarily modify spells you cast. To use an option, you must spend the number of Sorcery Points that it costs. You can use only one Metamagic option on a spell when you cast it, unless otherwise noted in one of those options. Whenever you gain a Sorcerer level, you can replace one of your Metamagic options with one you don’t know. You gain two more options at Sorcerer level 10 and two more at Sorcerer level 17.

                METAMAGIC OPTIONS
                The following options are available to your Metamagic features. The options are presented in alphabetical order.

                EMPOWERED SPELL
                Cost: 1 Sorcery Point

                When you roll damage for a spell, you can spend 1 Sorcery Point to reroll a number of the damage dice up to your Charisma modifier (minimum of one), and you must use the new rolls. You can use Empowered Spell even if you have already used a different Metamagic option during the casting of the spell.

                QUICKENED SPELL
                Cost: 2 Sorcery Points

                When you cast a spell that has a casting time of an action, you can spend 2 Sorcery Points to change the casting time to a Bonus Action for this casting. You can’t modify a spell in this way if you’ve already cast a spell of level 1 or higher on the current turn, nor can you cast a spell of level 1 or higher on this turn after modifying a spell in this way.
"
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                                    FeatureTraits {
                                        name: "Aasimar Traits".to_owned(),
                                        description: "Creature Type: Humanoid
Size: Medium (about 4–7 feet tall) or Small (about 2-4 feet tall), chosen when you select this species
Speed: 30 feet
Life Span: 160 years on average"
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                                    FeatureTraits {
                                        name: "Celestial Resistance".to_owned(),
                                        description: "You have resistance to Necrotic damage and Radiant damage."
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                                    FeatureTraits {
                                        name: "Darkvision".to_owned(),
                                        description: "Blessed with a radiant soul, your vision can easily cut through darkness. You have Darkvision with a range of 60 feet."
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                                    FeatureTraits {
                                        name: "Healing Hands".to_owned(),
                                        description: "As a Magic action, you can touch a creature and roll a number of d4s equal to your Proficiency Bonus. The creature regains a number of Hit Points equal to the total rolled. Once you use this trait you can’t use it again until you finish a Long Rest."
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: Some(0),
                                        max_charges: Some(1),
                                    },
                                    FeatureTraits {
                                        name: "Light Bearer".to_owned(),
                                        description: "You know the Light cantrip. Charisma is your spellcasting ability for it."
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: Some(0),
                                        max_charges: Some(1),
                                    },
                                ],
                locked_features_and_traits: vec![LockedFeatureTraits {
                    feature: FeatureTraits {
                                        name: "SORCERER SUBCLASS".to_owned(),
                                        description: "You gain a Sorcerer subclass of your choice:

Aberrant Sorcery
Clockwork Sorcery
Draconic Sorcery
Wild Magic Sorcery
Divine Soul Sorcery (Non-Playtest)
Shadow Sorcery (Non-Playtest)
Storm Sorcery (Non-Playtest)

Subclasses are detailed after this class’s description. A subclass is a specialization that grants you special features at certain Sorcerer levels. For the rest of your career, you gain each of your subclass’s features that are of your Sorcerer level and lower. There are non-playtest subclasses that can be used, please check with your DM before using one."
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                    unlock_level: Some(3),
                    unlock_condition: None,
                }, LockedFeatureTraits {
                    feature: FeatureTraits {
                                        name: "ABILITY SCORE IMPROVEMENT".to_owned(),
                                        description: "You gain the Ability Score Improvement feat or another feat of your choice for which you qualify. As shown on the Sorcerer table, you gain this feature again at levels 8, 12, 16.
                                        
General Feat (Prerequisite: Level 4+)

Increase one ability score of your choice by 2, or increase two ability scores of your choice by 1. This feat can’t increase an ability score above 20.

Repeatable. You can take this feat more than once."
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                    unlock_level: Some(4),
                    unlock_condition: None,
                }, LockedFeatureTraits {
                    feature: FeatureTraits {
                                        name: "SORCEROUS RESTORATION".to_owned(),
                                        description: "When you finish a Short Rest, you can regain expended Sorcery Points, but no more than a number equal to half your Sorcerer level (round down). Once you use this feature, you can't do so again until you finish a Long Rest."
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: Some(0),
                                        max_charges: Some(1),
                                    },
                    unlock_level: Some(5),
                    unlock_condition: None,
                }, LockedFeatureTraits {
                    feature: FeatureTraits {
                                        name: "SORCERY INCARNATE".to_owned(),
                                        description: "If you have no uses of Innate Sorcery left, you can use it if you spend 2 Sorcery Points when you take the Bonus Action to activate it. In addition, while your Innate Sorcery feature is active, you can use up to two of your Metamagic Options on each spell you cast."
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                    unlock_level: Some(5),
                    unlock_condition: None,
                }, LockedFeatureTraits {
                    feature: FeatureTraits {
                                        name: "EPIC BOON".to_owned(),
                                        description: "You gain an Epic Boon feat or another feat of your choice for which you qualify. Boon of Fate is recommended.
                                        
Boon of Combat Prowess
Epic Boon Feat (Prerequisite: Level 19+)

You gain the following benefits.

Ability Score Increase. Increase one ability score of your choice by 1, to a maximum of 30.

Peerless Aim. When you miss with an attack roll, you can hit instead. Once you use this benefit, you can’t use it again until the start of your next turn.

Boon of Dimensional Travel
Epic Boon Feat (Prerequisite: Level 19+)

You gain the following benefits.

Ability Score Increase. Increase one ability score of your choice by 1, to a maximum of 30.

Blink Steps. Immediately after you take the Attack action or the Magic action, you can teleport up to 30 feet to an unoccupied space you can see.

Boon of Fate
Epic Boon Feat (Prerequisite: Level 19+)

You gain the following benefits.

Ability Score Increase. Increase one ability score of your choice by 1, to a maximum of 30.

Improve Fate. When you or another creature within 60 feet of you succeeds on or fails a D20 Test, you can roll 2d4 and apply the total rolled as a bonus or penalty to the d20 roll. Once you use this benefit, you can’t use it again until you roll Initiative or finish a Short or Long Rest.

Boon of Irresistible Offense
Epic Boon Feat (Prerequisite: Level 19+)

You gain the following benefits.

Ability Score Increase. Increase your Strength or Dexterity score by 1, to a maximum of 30.

Overcome Defenses. The Bludgeoning, Piercing, and Slashing damage you deal always ignores Resistance.

Overwhelming Strike. When you roll a 20 on the d20 for an attack roll, you can deal extra damage to the target equal to the ability score increased by this feat. The extra damage’s type is the same as the attack’s type.

Boon of the Night Spirit
Epic Boon Feat (Prerequisite: Level 19+)

You gain the following benefits.

Ability Score Increase. Increase one ability score of your choice by 1, to a maximum of 30.

Merge with Shadows. While within Dim Light or Darkness, you can give yourself the Invisible condition as a Bonus Action. The condition ends on you immediately after you take an action, a Bonus Action, or a Reaction.

Shadowy Form. While within Dim Light or Darkness, you have Resistance to all damage except Psychic and Radiant.

Boon of Spell Recall
Epic Boon Feat (Prerequisite: Level 19+, Spellcasting Feature)

You gain the following benefits.

Ability Score Increase. Increase your Intelligence, Wisdom, or Charisma score by 1, to a maximum of 30.

Free Casting. Whenever you cast a spell with a level 1–4 spell slot, roll 1d4. If the number you roll is the same as the slot’s level, the slot isn’t expended.

Boon of Truesight
Epic Boon Feat (Prerequisite: Level 19+)

You gain the following benefits.

Ability Score Increase. Increase one ability score of your choice by 1, to a maximum of 30.

Truesight. You have Truesight with a range of 60 feet."
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                    unlock_level: Some(19),
                    unlock_condition: None,
                }, LockedFeatureTraits {
                    feature: FeatureTraits {
                                        name: "ARCANE APOTHEOSIS".to_owned(),
                                        description: "While your Innate Sorcery feature is active, you can use one Metamagic Option on each of your turns without expending Sorcery Points on it"
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: None,
                                        max_charges: None,
                                    },
                    unlock_level: Some(20),
                    unlock_condition: None,
                }, LockedFeatureTraits {
                    feature: FeatureTraits {
                                        name: "Celestial Revelation".to_owned(),
                                        description: "When you reach character 3rd level, you can transform as a Bonus Action using one of the options below (choose the option each time you transform). The transformation lasts for 1 minute or until you end it (no action required). Once you transform, you can't do so again until you finish a Long Rest. Once on each of your turns before the transformation ends, you can deal extra damage to one target when you deal damage to it with an attack or spell. The extra damage equals your Proficiency Bonus, and the extra damage's type is either Necrotic for Necrotic Shroud or Radiant for Heavenly Wings and Inner Radiance.
Heavenly Wings. Two spectral wings sprout from your back temporarily. Until your transformation ends, you have a Flying Speed equal to your Speed.
Inner Radiance. Searing light temporarily radiates from your eyes and mouth. For the duration, you shed Bright Light in a 10-foot radius and Dim Light for an additional 10 feet, and at the end of each of your turns, each creature within 10 feet of you takes Radiant damage equal to your Proficiency Bonus.
Necrotic Shroud. Your eyes turn into pools of darkness and flightless wings sprout from your back temporarily. Creatures other than your allies within 10 feet of you must each succeed on a Charisma saving throw (DC 8 + your proficiency bonus + your Charisma modifier) or have the Frightened condition until the end of your next turn."
                                            .to_owned(),
                                        duration: None,
                                        trigger: None,
                                        cooldown: None,
                                        used_charges: Some(0),
                                        max_charges: Some(1),
                                    },
                    unlock_level: Some(3),
                    unlock_condition: None,
                }],
            },
            notes: Notes {
                organizations: None,
                allies: None,
                enemies: None,
                backstory: "Pearl is a rank P45 senior manager of the Strategic Investment \
            Department in the Interastral Peace Corporation, a member of the Ten Stonehearts, \
            the leader of Pearluxe Corp, and the CEO of Planarcadia."
                    .to_owned(),
                other: None,
            },
        }
    }

    #[tokio::test]
    async fn test_add_spells() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;

        let res = gemini_service
            .add_character_spells(&message_sender, "1483098634601107486", "anyTHING")
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                &message_sender,
                "1483098634601107486",
                "anyTHING",
                "我目前会光亮术、寒冰射线和魔法飞弹，没有其他的了，用的魅力值作为判定属性，魅力加值3，熟练加值2，DC 13，一环法术位4个二环法术位2个，无消耗",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(&message_sender, "1483098634601107486", "anyTHING", "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service
            .get_character("1483098634601107486")
            .await?;

        assert_json_snapshot!(character.magic);

        assert!(gemini_service.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_judge() -> Result<(), Box<dyn std::error::Error>> {
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let message_sender = MockMessageSender;

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "系统提示：角色创建完成。
        你发现自己站在一间昏暗的酒馆里，空气中弥漫着酒精味。",
            )
            .await?;

        assert_eq!(response, "收到，规则已就绪。作为审计员，我已记录本次初始设置。

        鉴于目前处于角色创建完成阶段，请 DM 告知相关角色的名称，以便我将该角色关联至后续的审计流程中，并建立初始状态记录。");

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "街角传来骚动，似乎有人在议论最近失踪的旅人……",
            )
            .await?;

        assert_eq!(response, "收到，地下城主。我已经记录当前的剧情背景，并随时准备对涉及游戏规则、角色数据变动或判定逻辑的内容进行审核。

        目前该消息属于剧情描述与氛围铺陈，不涉及具体的数值变更、角色判定或物品管理，因此无需进行强制性规则校验。请继续您的叙述或发起后续的判定。");

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "泽阿里尔正在整理装备，而庄芳宜已经走进酒馆开始喝酒。",
            )
            .await?;

        assert_eq!(response, "好的，我已收到关于角色“泽阿里尔”和“庄芳宜”的状态更新。

        根据你的指令，我将对以上叙述进行规则审计：

        1.  **动作审核**：
            *   **泽阿里尔**：“整理装备”。这是一个叙述性的动作，不涉及数值改动或规则判定。
            *   **庄芳宜**：“走进酒馆开始喝酒”。同样属于角色扮演的叙述，不涉及硬性规则冲突，但请注意，如果在饮酒过程中伴随有酒量检定（如体质豁免）或特殊消耗（金币购买），请在后续的对话中具体说明，届时我将对该操作进行规则验证。

        2.  **数据变更**：
            *   当前消息不涉及任何数据修改（如物品增减、属性变动等），因此无需调用修改工具。

        **审计结论**：该描述符合一般游戏流程，无规则违规，可以继续进行。请在接下来的游戏进程中，如果有具体的规则判定请求（如检定、战斗、物品交易），请及时告知。");

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "庄芳宜使用了大剑斩杀酒馆混混，造成了不小的骚动。",
            )
            .await?;

        assert_eq!(response, "关于“庄芳宜使用大剑斩杀酒馆混混”这一行为，由于数据库中未能查找到名为“庄芳宜”的角色卡，我无法对其当前的数据状态（如物品属性、等级及行为能力）进行具体的规则校验。

        请确认：
        1. 该角色是否已登记在册？（名称是否存在拼写错误）
        2. 如果该角色为临时NPC或玩家角色，请提供其角色卡相关数据以便我进行后续的合规性审计。

        此外，作为规则审计员，提醒DM注意：在《龙与地下城》规则中，**“斩杀”**（即直接导致角色死亡）通常涉及复杂的战斗判定与伤害计算。请确保在叙述该行为时，已遵循战斗规则中的攻击检定、伤害投掷及死亡判定程序。");

        let test_character = test_character();

        character_sheet_service
            .upsert_character(test_character)
            .await?;

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "真珠刚用毛笔画（变）出了一头牛击退了敌人",
            )
            .await?;

        assert_eq!(response, "审核员报告：

        针对“真珠”使用毛笔画出牛并击退敌人的描述，根据其角色卡（术士，2级）及 D&D 规则进行如下审计：

        1. **规则可行性分析**：
            *   **法术/能力判定**：真珠的角色卡中目前已知法术仅包含《法术戏法》和《魔法飞弹》，没有名为“画物成真”或类似召唤牛只的法术。
            *   **职业特性**：术士并未直接拥有“通过绘画创造生物”的职业能力。
            *   **动作判定**：若该行为被定义为法术，则应属于消耗法术位（或其他资源）的动作；若作为戏法，则需满足特定的法术列表规则。

        2. **审计结论**：
            *   该描述的行为目前无法在现有角色数据规则中找到对应的法术或特性支持，存在规则疑点。
            *   若 DM 希望保留此设定，请确认该行为是否属于某种特制物品（例如特殊魔法道具）、法术重写（Flavor text/reskinning）或 DM 的额外叙事性裁定。

        3. **操作建议**：
            *   如该行为意在替代某个现有法术，建议说明其对应的法术名称与消耗的资源（如消耗法术位）。
            *   在未明确其规则来源前，暂不对该角色的数值进行变更。请 DM 补充说明该行动的规则逻辑，以便后续核准。");

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "真珠获得了400点经验值，升级了",
            )
            .await?;

        assert_eq!(response, "审核结果：

        1.  **当前信息核对**：角色“真珠”（Discord ID: 400114655500042240）目前等级为2级，经验值为330。
        2.  **经验值校验**：DM 提到增加 400 点经验值。330 + 400 = 730。根据 D&D 5E 标准升级表，从 2 级升至 3 级需要达到 900 经验值。
        3.  **结论**：
            *   经验值变动后的总额（730）未达到 3 级所需的 900 点阈值。
            *   因此，**当前不支持“升级”这一判定**。

        **修正建议**：
        请 DM 核对所发放的经验值数值，或者确认是否采用了自定义升级规则。若按照标准规则，角色“真珠”目前仍应保持在 2 级。本次数据修改（升级）暂不予执行。");

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "真珠获得了1000点经验值，升级了，学习了新法术冰霜世界",
            )
            .await?;

        assert_eq!(response, "审核结果：

        1. **经验值与等级变动**：
           - 角色“真珠”当前等级为 2 级，经验值为 330。
           - DM 提出增加 1000 经验值。根据 5e 标准规则，从 2 级升级至 3 级需要 900 累计经验值（目前角色已有 330，符合升级条件）。
           - **审核结论**：升级请求符合规则，准予更新。

        2. **新法术学习**：
           - DM 提到学习新法术“冰霜世界”。在 D&D 规则手册中，未查到名为“冰霜世界”的标准法术。
           - **审核结论**：请 DM 核实该法术名称是否为自定义内容（Homebrew）或误称（如原名是否为 *Frostbite* 等）。若为自定义法术，请提供其详细效果（环阶、施法时间、射程、成分、持续时间及效应），以便评估其平衡性。

        **目前状态**：
        - 由于法术名称存在疑义，**暂不执行数据库更新**。请 DM 补充说明“冰霜世界”的具体规则细节，待审核确认后，我将为您执行等级及法术库的同步更新。");

        let updated_character = character_sheet_service
            .get_character_by_name("真珠")
            .await?;

        assert_eq!(updated_character.progression.level, 2);

        let response = gemini_service
            .request_to_llm(
                &message_sender,
                "dnd-5",
                "1483098634601107476",
                "真珠获得了1000点经验值，升级了，学习了新法术蛛网术，新最大HP为22",
            )
            .await?;

        assert_eq!(response, "【事务状态】SUCCESS

        【角色】
        - 名称：真珠 (Pearl)
        - 当前等级：3
        - 当前职业：术士 (Sorcerer - Draconic Bloodline: Gold)

        【事务步骤】
        1. **经验值变更**：增加 1000 XP，当前总经验值 1330 XP。
        2. **等级提升**：经验值满足升级条件，角色从 2 级提升至 3 级。
        3. **属性推演**：
            - **生命值 (HP)**：增加 7 点（固定值 4 + 体质 2 + 龙族血脉 1），最大生命值更新为 23。当前生命值同步补满。
            - **法术位**：获得 2 环法术位（2个），1 环法术位增加至 4 个。
            - **术法点 (Sorcery Points)**：上限提升至 3 点。
        4. **法术学习**：习得 2 环法术《蛛网术 (Web)》。
        5. **超魔选择**：选择了“远程法术 (Distant Spell)”和“瞬发法术 (Quickened Spell)”。
        6. **种族特性更新**：作为 Aasimar，达到 3 级自动解锁《次级恢复术 (Lesser Restoration)》（每日一次）。

        【自动推导结果】
        - **最大 HP**：16 -> 23
        - **法术位**：1环 [4], 2环 [2]
        - **术法点上限**：3
        - **生命骰 (Hit Dice)**：增加 1d6，总计 3d6（数据库显示仍为 2d6，已记录待同步）
        - **种族特性**：解锁《天界传承：次级恢复术》

        【玩家待选择项】
        - 无（所有关键选择已在消息中明确）

        【规则审核结果】
        - **合法性确认**：
            - 术士 3 级允许习得第 4 个已知法术，且允许学习 2 环法术。《蛛网术》合法。
            - 术士 3 级解锁超魔特性，数量为 2 个。所选超魔选项合法。
            - 经验值计算正确。

        【数据库写入状态】已完成核心数据（等级、XP、HP、法术位、已知法术）的提交。

        【最终结论】
        事务已成功提交。真珠现已正式升至 3 级。由于系统限制，部分文本描述类特性（如超魔描述、生命骰文本）建议在角色卡备注中手动同步，核心数值已更新完毕。");

        Ok(())
    }

    #[tokio::test]
    async fn test_new_summary() -> Result<(), Box<dyn std::error::Error>> {
        let (mut gemini_service, pool, _, _) = service_setup().await?;
        let message_sender = MockMessageSender;

        gemini_service.new_summary(&message_sender).await?;

        let response: Vec<StoryEntity> = sqlx::query_as("SELECT * FROM story")
            .fetch_all(&pool)
            .await?;

        assert_eq!(response.len(), 0);

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' 系统提示：角色创建完成。
你发现自己站在一间昏暗的酒馆里，空气中弥漫着酒精味。', 'dnd-5', 'Dungeon Master', '1483098634601107476', '2026-05-07 02:37:56.756385+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' 如果我攻击哥布林会发生什么？我拔出剑冲上去。', 'shaomo1', 'Unknown Adventurer - shaomo1', '1483098634601107489', '2026-05-07 02:38:52.373598+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' 街角传来骚动，似乎有人在议论最近失踪的旅人……', 'dnd-5', 'Dungeon Master', '1483098634601107476', '2026-05-07 02:39:22.069725+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' 泽阿里尔正在整理装备，而庄芳宜已经走进酒馆开始喝酒。', 'dnd-5', 'Dungeon Master', '1483098634601107476', '2026-05-07 02:40:55.613852+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' （系统正在初始化）酒馆里人声鼎沸，冒险者们在讨论最近的失踪事件。', 'dnd-5', 'Dungeon Master', '1483098634601107476', '2026-05-07 02:41:29.238675+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' 我决定先喝酒', 'anyTHING', '庄芳宜', '1483098634601107486', '2026-05-07 02:41:56.587162+00')",
            )
            .execute(&pool)
            .await?;

        sqlx::query(
                "INSERT INTO public.dialogues (dialogue, author_name, author_character, author_discord_id, updated_at) VALUES (' 喝酒。', 'anyTHING', '庄芳宜', '1483098634601107486', '2026-05-07 02:42:19.420479+00')",
            )
            .execute(&pool)
            .await?;

        gemini_service.new_summary(&message_sender).await?;

        let response: Vec<StoryEntity> = sqlx::query_as("SELECT * FROM story")
            .fetch_all(&pool)
            .await?;

        assert_json_snapshot!(response, {
            "[].updated_at" => "[timestamp]",
        });
        Ok(())
    }

    #[tokio::test]
    async fn test_new_dialogue() -> Result<(), Box<dyn std::error::Error>> {
        let (mut gemini_service, pool, _, _) = service_setup().await?;
        let message_sender = MockMessageSender;

        sqlx::query_as::<_, CharacterSheet>(
            r#"
        INSERT INTO character_sheets (
            id,
            meta,
            identity,
            progression,
            combat,
            abilities_block,
            skills,
            magic,
            inventory,
            traits,
            notes
        )
        VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
        ON CONFLICT (id) DO UPDATE SET
            identity = EXCLUDED.identity
        RETURNING *
        "#,
        )
        .bind("1483098634601107486")
        .bind(Meta::default())
        .bind(Identity {
            character_name: "庄芳宜".to_string(),
            species: "人类".to_string(),
            sub_species: Some("人类".to_string()),
            class: "战士".to_string(),
            sub_class: Some("剑士".to_string()),
            characteristics: Characteristics::default(),
        })
        .bind(Progression::default())
        .bind(Combat::default())
        .bind(AbilitiesBlock::default())
        .bind(Skills::default())
        .bind(Spells::default())
        .bind(Inventory::default())
        .bind(Traits::default())
        .bind(Notes::default())
        .fetch_one(&pool)
        .await?;

        gemini_service
            .store_new_dialogue(&message_sender, "", "", "")
            .await?;

        let response: Vec<DialogueEntity> = sqlx::query_as("SELECT * FROM dialogues")
            .fetch_all(&pool)
            .await?;

        assert_eq!(response.len(), 0);

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(&message_sender, "当前在建卡步骤：选择职业。可回复：我选战士 / 我选游荡者 / 我选法师 / 我选牧师 / 我选游侠 / 我选圣武士。
如果要退出，回复：取消建卡。", "1483098634601107476", "dnd-5")
            .await?;

        let response: Vec<DialogueEntity> = sqlx::query_as("SELECT * FROM dialogues")
            .fetch_all(&pool)
            .await?;

        assert_eq!(response.len(), 0);

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "当前在建卡步骤：选择职业，但如果你已经决定背景，可以直接说“我在酒馆喝酒”进入剧情。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "系统提示：角色创建完成。
你发现自己站在一间昏暗的酒馆里，空气中弥漫着酒精味。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "我可能会考虑去酒馆看看情况，但还没决定。",
                "1483098634601107489",
                "shaomo1",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "如果我攻击哥布林会发生什么？我拔出剑冲上去。",
                "1483098634601107489",
                "shaomo1",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "街角传来骚动，似乎有人在议论最近失踪的旅人……",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "你现在可以自由探索城镇，酒馆就在前方。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "三天后，你已经离开村庄，踏上前往北方的道路。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "泽阿里尔正在整理装备，而庄芳宜已经走进酒馆开始喝酒。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "（系统正在初始化）酒馆里人声鼎沸，冒险者们在讨论最近的失踪事件。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "我决定先喝酒",
                "1483098634601107486",
                "anyTHING",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(&message_sender, "喝酒。", "1483098634601107486", "anyTHING")
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                &message_sender,
                "我退出游戏，不再继续剧情。",
                "1483098634601107486",
                "anyTHING",
            )
            .await?;

        let response: Vec<DialogueEntity> = sqlx::query_as("SELECT * FROM dialogues")
            .fetch_all(&pool)
            .await?;

        assert_json_snapshot!(response, {
            "[].updated_at" => "[timestamp]",
        });
        Ok(())
    }
}

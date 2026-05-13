use std::collections::VecDeque;

use async_trait::async_trait;
use rig::{completion::Completion, message::Message, tool::ToolDyn};
use tracing::info;

use crate::{
    llm::{Cache, Llm, LlmCore, ToolFactory, error::LlmError},
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

        let mut memory = vec![];

        let reply = self
            .core()
            .completion_with_retry(|| async {
                agent
                    .completion(message.clone(), memory.clone())
                    .await?
                    .send()
                    .await
            })
            .await?;

        memory.push(message.into());
        memory.push(reply.choice.clone().into());

        let mut texts = vec![];
        let mut tool_calls = VecDeque::new();
        self.core()
            .collect_response(&reply, &mut texts, &mut tool_calls);

        while let Some(tool_call) = tool_calls.pop_front() {
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
                        .completion(message.clone(), memory.clone())
                        .await?
                        .send()
                        .await
                })
                .await?;

            memory.push(message);
            memory.push(followup.choice.clone().into());

            self.core()
                .collect_response(&followup, &mut texts, &mut tool_calls);
        }

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

        if !tool_calls.is_empty() {
            let character = self
                .core()
                .character_sheet_service
                .get_character(&author_id)
                .await;
            let author_character = match character {
                Ok(character) => character.identity.character_name,
                _ => format!("Unknown Adventurer - {}", author_name),
            };
            self.core()
                .story_service
                .insert_new_dialogue(message, author_name, &author_character, author_id)
                .await?;
        }
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

    fn remove_cache_impl(&mut self, discord_user_id: &str) {
        self.core_mut().remove_cache(discord_user_id)
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

            fn remove_cache(&mut self, discord_user_id: &str) {
                self.remove_cache_impl(discord_user_id)
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

#[cfg(test)]
mod test {
    use std::{collections::HashMap, fs, sync::Arc, thread::sleep, time::Duration};

    use chrono::Utc;
    use insta::assert_json_snapshot;
    use sqlx::{Pool, Postgres};

    use crate::{
        character::{
            entity::{
                CharacterSheet,
                abilities_block::AbilitiesBlock,
                combat::Combat,
                identity::{Characteristics, Identity},
                inventory::Inventory,
                meta::Meta,
                notes::Notes,
                progression::Progression,
                skills::Skills,
                spells::Spells,
                traits::Traits,
            },
            repository::CharacterSheetRepository,
            service::CharacterSheetService,
        },
        config::{AiDmConfig, ServiceConfig},
        llm::{Gemini, Llm, LlmCore},
        pg_pool::{TestPgPool, TestPgPoolConfig},
        story::{
            entity::{DialogueEntity, StoryEntity},
            repository::{DialogueRepository, StoryRepository},
            service::StoryService,
        },
        tool::service::ToolService,
    };

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
                core: LlmCore {
                    model: "gemini-3.1-flash-lite-preview".to_owned(),
                    story_service,
                    character_sheet_service: character_sheet_service.clone(),
                    cached_context: HashMap::new(),
                    dm_discord_id: "1483098634601107476".to_owned(),
                    folder_path: "./prompts".to_string(),
                    compile_trigger: 4,
                    tool_service,
                    base_url: None,
                    retry_attempt: 20,
                },
            },
            pool,
            db_name,
            character_sheet_service,
        ))
    }

    fn test_character() -> Result<CharacterSheet, Box<dyn std::error::Error>> {
        let character_json = fs::File::open("./src/llm/test_character/test_character.json")?;
        let character: CharacterSheet = serde_json::from_reader(character_json)?;
        Ok(character)
    }

    #[tokio::test]
    async fn test_add_spells() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;

        let res = gemini_service
            .add_character_spells("1483098634601107486")
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                "1483098634601107486",
                "我目前会光亮术、寒冰射线和魔法飞弹，没有其他的了，用的魅力值作为判定属性，魅力加值3，熟练加值2，DC 13，一环法术位4个二环法术位2个，无消耗",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue("1483098634601107486", "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service
            .get_character("1483098634601107486")
            .await?;

        assert_json_snapshot!(character.magic);

        assert!(gemini_service.core.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_meta() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let discord_id = "1483098634601107487";

        let res = gemini_service.add_character_meta(discord_id).await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                discord_id,
                "我的位置是银月城酒馆，剧情摘要是刚加入冒险队伍，没有额外生物，没有死亡，正在喝茶，行动结束时间是Harptos 24 Mar 1555 12:35PM",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(discord_id, "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.meta);
        assert!(gemini_service.core.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_identity() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let discord_id = "1483098634601107488";

        let res = gemini_service.add_character_identity(discord_id).await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                discord_id,
                "角色名叫艾琳，是高等精灵，职业法师，子职业防护学派，背景是贤者，背景特性是研究员，阵营守序善良，女性，蓝眼，体型中型，身高5尺6寸，信仰密斯特拉，银发，白皙皮肤，年龄120岁，体重110磅，性格特质是喜欢记录所有奥秘，理念是知识应被守护，牵绊是导师留下的法典，缺陷是过度好奇，外貌特征有银色长发和蓝色长袍",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(discord_id, "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.identity);
        assert!(gemini_service.core.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_progression() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let discord_id = "1483098634601107489";

        let res = gemini_service.add_character_progression(discord_id).await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                discord_id,
                "角色等级3级，经验900，总生命骰3d8，最大生命值24，熟练加值2，护甲熟练轻甲和中甲，武器熟练长剑和短弓，工具熟练盗贼工具，语言通用语和精灵语",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(discord_id, "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.progression);
        assert!(gemini_service.core.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_combat() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let discord_id = "1483098634601107490";
        let mut character = test_character()?;
        character.meta.discord_id = discord_id.to_owned();
        character_sheet_service.upsert_character(character).await?;

        let res = gemini_service.add_character_combat(discord_id).await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                discord_id,
                "护甲等级16，先攻2，当前生命值20，速度步行30尺，感官黑暗视觉60尺，抗性火焰，免疫无，易伤无，没有状态，力竭0级，豁免熟练体质和魅力，动作有攻击、冲刺、撤离，战斗动作有长剑命中加5伤害1d8+3、短弓命中加4伤害1d6+2",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(discord_id, "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.combat);
        assert!(gemini_service.core.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_inventory() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let discord_id = "1483098634601107491";

        let res = gemini_service.add_character_inventory(discord_id).await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                discord_id,
                "物品栏有长剑一把，重量3，价值15金币，已装备；治疗药水2瓶，每瓶重量1，价值50金币，未装备；旅行者衣服一套，重量4，价值2金币，未装备",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(discord_id, "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.inventory);
        assert!(gemini_service.core.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_abilities() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let discord_id = "1483098634601107492";

        let res = gemini_service.add_character_abilities(discord_id).await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                discord_id,
                "力量10，敏捷14，体质12，智力16，感知13，魅力8，所有额外修正值都是0",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(discord_id, "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.abilities_block);
        assert!(gemini_service.core.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_skills() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let discord_id = "1483098634601107493";
        let mut character = test_character()?;
        character.meta.discord_id = discord_id.to_owned();
        character_sheet_service.upsert_character(character).await?;

        let res = gemini_service.add_character_skills(discord_id).await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                discord_id,
                "技能熟练项是奥秘、历史、调查、察觉，其他技能不熟练。所有技能的属性按DND默认：运动力量，体操敏捷，巧手敏捷，隐匿敏捷，奥秘历史调查自然宗教智力，驯兽洞悉医药察觉生存感知，欺瞒威吓表演游说魅力。被动值使用默认0",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(discord_id, "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.skills);
        assert!(gemini_service.core.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_traits() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let discord_id = "1483098634601107494";

        let res = gemini_service.add_character_traits(discord_id).await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                discord_id,
                "已解锁特性有黑暗视觉，描述是在微光中视为明亮、黑暗中视为微光，距离60尺；精类血统，描述是魅惑豁免有优势且不能被魔法睡眠。锁定特性有额外攻击，描述是每次攻击动作可攻击两次，5级解锁，没有其他解锁条件",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(discord_id, "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.traits);
        assert!(gemini_service.core.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_add_notes() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;
        let discord_id = "1483098634601107495";

        let res = gemini_service.add_character_notes(discord_id).await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(
                discord_id,
                "组织是银月法师会，盟友是导师赛琳，敌人是红袍巫师，背景故事是从烛堡出发寻找失落星图，其他备注是喜欢收集古代硬币",
            )
            .await?;

        println!("Response: {res}");

        let res = gemini_service
            .conversation_continue(discord_id, "确认")
            .await?;

        println!("Response: {res}");

        let character = character_sheet_service.get_character(discord_id).await?;

        assert_json_snapshot!(character.notes);
        assert!(gemini_service.core.cached_context.is_empty());

        Ok(())
    }

    #[tokio::test]
    async fn test_judge() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, _pool, _, character_sheet_service) = service_setup().await?;

        let response = gemini_service
            .request_to_llm(
                "1483098634601107476",
                "系统提示：角色创建完成。
        你发现自己站在一间昏暗的酒馆里，空气中弥漫着酒精味。",
            )
            .await?;

        // 收到，规则审核器已就绪。

        // 目前的角色卡数据库为空。请 DM 提供需要载入的角色卡信息（名称、属性、等级、生命值、物品、法术等），以便我建立初始数据库并开始进行规则审核。

        // 在获得角色卡数据之前，我无法执行任何关于角色状态的校验或推导。

        // 请提供详细的角色信息。
        println!("Response: {response}");

        let response = gemini_service
            .request_to_llm(
                "1483098634601107476",
                "街角传来骚动，似乎有人在议论最近失踪的旅人……",
            )
            .await?;

        // 收到。作为规则审核器与数据库维护者，我已记录当前剧情背景。

        // 由于该信息属于叙事描述，不涉及具体数值变更、资源消耗或规则判定，因此：

        // ## 审核结果
        // * 合法（叙事信息）

        // ## 规则依据
        // * 无需规则判定

        // ## 状态变更分析
        // * 无数值变动

        // ## 数据库操作
        // * 无需更新

        // ## 风险警告
        // * 无

        // ---
        // **当前状态：** 等待 DM 或玩家发出涉及角色数据、战斗状态、物品、法术或资源交互的明确指令。
        println!("Response: {response}");

        let response = gemini_service
            .request_to_llm(
                "1483098634601107476",
                "泽阿里尔正在整理装备，而庄芳宜已经走进酒馆开始喝酒。",
            )
            .await?;

        // ## 审核结果

        // * **信息不足**

        // ## 规则依据

        // * 角色名未在系统数据库中找到对应角色卡。

        // ## 状态变更分析

        // * DM 描述了“泽阿里尔正在整理装备”和“庄芳宜走进酒馆开始喝酒”。
        // * 由于无法查询到这两名角色的角色卡，无法验证这两位角色当前是否持有相关装备、是否拥有金币进行消费、或是否存在其他状态影响。

        // ## 数据库操作

        // * **禁止更新**。原因：系统无法识别角色名“泽阿里尔”与“庄芳宜”。

        // ## 风险警告

        // * **角色不存在**：系统中没有这两个角色的登记信息。请 DM 确认角色名是否正确，或者是否需要先为这两位角色录入数据。
        // * **数据缺失**：在缺乏角色卡的情况下，无法进行任何规则校验或资源（如金币、装备）扣除操作。

        // ---

        // **请提供准确的角色名称或确认角色是否已录入系统。如果这是新角色，请补充角色卡信息。**
        // ## 审核结果

        // * **信息不足**

        // ## 规则依据

        // * 数据库未查找到 Discord ID 为 `1483098634601107476` 的用户信息。
        // * 系统中不存在名为“泽阿里尔”或“庄芳宜”的角色卡。

        // ## 状态变更分析

        // * DM 提到的“整理装备”和“喝酒”行为可能涉及物品消耗、金币减少或状态变更。
        // * 由于缺乏基础角色数据，无法进行任何有效的规则判定与状态审计。

        // ## 数据库操作

        // * **禁止更新**。原因：系统内无相关角色及玩家记录，无法执行任何变更。

        // ## 风险警告

        // * **数据缺失**：当前环境缺失必要的角色存档。请先创建或登记角色信息，否则我无法履行审计职能。

        // ---

        // **请提供角色的详细资料，以便我将其录入数据库并进行后续的规则审计与状态管理。**
        println!("Response: {response}");

        let response = gemini_service
            .request_to_llm(
                "1483098634601107476",
                "庄芳宜使用了大剑斩杀酒馆混混，造成了不小的骚动。",
            )
            .await?;

        // ## 审核结果

        // * 信息不足

        // ## 规则依据

        // * 根据 DND 5.5e 规则及角色数据一致性原则，任何战斗行为的结算（如攻击判定、伤害计算、资源消耗、状态异常等）必须基于确定的角色属性及装备定义。

        // ## 状态变更分析

        // * DM 提到角色“庄芳宜”使用了“大剑”，但系统未能检索到名为“庄芳宜”的角色卡。
        // * 无法确认该角色是否持有“大剑”。
        // * 无法确认该角色的各项基础数值（如攻击加值、力量属性等）。
        // * 无法判断战斗结果对角色状态（如是否存在负面影响、消耗或触发特性）的影响。

        // ## 数据库操作

        // * 禁止更新。
        // * 原因：系统中不存在名为“庄芳宜”的角色卡，无法验证操作合法性及进行相关状态更新。

        // ## 风险警告

        // * 角色缺失：由于未找到该角色，当前无法跟踪其任何生命值、资源或物品状态。
        // * 非标准流程：请提供该角色的完整信息，或确认其是否已在数据库中录入（如有别名请注明）。

        // ---
        // **请补充说明：**
        // 1. 请核实该角色名称是否正确。
        // 2. 若该角色为新角色，请提供其基础数据（属性、装备、生命值等）以便创建角色卡。
        println!("Response: {response}");

        let test_character = test_character()?;

        character_sheet_service
            .upsert_character(test_character)
            .await?;

        let response = gemini_service
            .request_to_llm(
                "1483098634601107476",
                "真珠刚用毛笔画（变）出了一头牛击退了敌人",
            )
            .await?;

        // ## 审核结果

        // * **非法**

        // ## 规则依据

        // * 根据角色卡，真珠（Sorcerer, Level 2）所掌握的已知法术包括：*Light, Message, Minor Illusion, Prestidigitation, Ray of Frost*（戏法）以及 *Magic Missile, Shield, Thunderwave*（1环）。
        // * 角色卡中并未记录有任何“用毛笔画出一头牛”或类似创造实体生物/召唤生物的法术或特性。
        // * 在D&D 5.5e规则中，除非通过特定法术（如*Find Familiar*, *Summon Beast*等）或职业特性，否则无法仅凭“毛笔画画”直接创造出具有击退能力的实体牛。

        // ## 状态变更分析

        // * 本次描述涉及：未被角色卡定义的魔法效果（画牛）。
        // * 缺失的数据：该行为是否消耗了资源？是否有判定需求（如法术攻击、豁免或检定）？是否有相应的法术描述作为依据？

        // ## 数据库操作

        // * **禁止更新**。
        // * 原因：DM描述的行为超出了角色卡定义的能力范围，且未提供该效果的规则来源。

        // ## 风险警告

        // * **未定义内容**：角色卡中没有该能力的记录。
        // * **规则冲突**：如果不基于已知的法术列表或特性描述，此类行为属于违规操作。
        // * **进一步确认**：如果这是DM设定的Homebrew效果或特殊剧情物品，请提供完整的规则描述（例如：消耗几级法术位？持续时间？效果范围？DC是多少？），以便我将其录入角色卡并进行后续合规性校验。
        println!("Response: {response}");

        let response = gemini_service
            .request_to_llm("1483098634601107476", "真珠获得了400点经验值，升级了")
            .await?;

        // ## 审核结果

        // * **非法**

        // ## 规则依据

        // * 根据角色卡，真珠当前等级为 2，经验值为 330。
        // * 依据 DND 5.5e 升级规则及真珠的当前职业（术士），术士升至 3 级所需的总经验值为 900。
        // * 玩家描述其获得了 400 点经验值，更新后总经验应为 330 + 400 = 730。
        // * 730 < 900，因此真珠尚未达到升至 3 级的经验阈值。

        // ## 状态变更分析

        // * **尝试修改的数据：** 等级、经验值。
        // * **判定：** 经验值不足，无法升级。

        // ## 数据库操作

        // * **禁止更新。**
        // * **原因：** 真珠获得的经验值不足以使其从 2 级提升至 3 级。

        // ## 风险警告

        // * 玩家对于升级条件的理解与当前角色数据不符，建议 DM 核实经验值计算方式或确认当前等级经验需求。
        println!("Response: {response}");

        let response = gemini_service
            .request_to_llm(
                "1483098634601107476",
                "真珠获得了1000点经验值，升级了，学习了新法术冰霜世界",
            )
            .await?;

        // ## 审核结果

        // * **非法**

        // ## 规则依据

        // 1.  **等级逻辑冲突**：角色卡显示真珠当前等级为 **2级**。根据升级规则，从2级升至3级需要经验值，但玩家/DM声称获得1000经验值并升级。根据D&D 5e标准升级表，2级升至3级需要积累到900XP（总计），当前真珠经验值为330，获得1000XP后总计应为1330XP，这确实达到了3级（900XP）的要求。
        // 2.  **法术学习合法性**：玩家提到学习新法术“冰霜世界”（Frost World）。
        //     *   在《玩家手册》（PHB）或《规则书》中不存在名为“冰霜世界”的官方Sorcerer法术。
        //     *   此法术疑似为 **Homebrew（自制）内容** 或 **非标准法术**。
        // 3.  **信息缺失**：由于“冰霜世界”是自定义法术，角色卡中未记录该法术的详细信息（施法时间、距离、效果、等级等），数据库无法推导该法术数据。

        // ## 状态变更分析

        // *   **拟修改数据**：等级（2->3）、经验值（330->1330）、法术列表（新增“冰霜世界”）。
        // *   **连带影响数据**：
        //     *   等级提升后，Sorcerer 3级会获得 **Metamagic（超魔）** 特性。
        //     *   等级提升后，法术位会增加（新增2环法术位）。
        //     *   Sorcerer 3级会解锁子职业（Subclass）特性（当前角色卡已有定义，但需确认是否需要进行选择）。
        // *   **缺失数据**：“冰霜世界”的完整法术定义。

        // ## 数据库操作

        // *   **禁止更新**：
        //     1.  由于自定义法术“冰霜世界”缺乏定义，无法录入数据库。
        //     2.  根据规则，3级术士需要选择“超魔”选项，玩家未说明。
        //     3.  3级需要确认子职业的各项细节（尽管角色卡标注了Draconic Bloodline，但需确认为升级后的状态）。

        // ## 风险警告

        // *   **Homebrew内容**：“冰霜世界”属于未定义法术，必须由DM补充详细属性（施法时间、耗材、射程、持续时间、效果说明、等级等）。
        // *   **规则冲突**：若DM强行指定该法术，需明确该法术的等级（由于它是升级获得的，必须匹配对应的法术环位）。

        // ---

        // **请补充以下信息以继续处理：**
        // 1. 请DM提供法术 **“冰霜世界”** 的完整定义（等级、施法时间、范围、持续时间、效果、是否需要专注等）。
        // 2. 请确认升级后真珠选择的 **两个超魔选项**（Metamagic Options）。
        println!("Response: {response}");

        let updated_character = character_sheet_service
            .get_character_by_name("真珠")
            .await?;

        assert_eq!(updated_character.progression.level, 2);

        let response = gemini_service
            .request_to_llm(
                "1483098634601107476",
                "真珠获得了1000点经验值，升级了，学习了新法术蛛网术，新最大HP为22",
            )
            .await?;

        // ## 审核结果

        // * **非法**

        // ## 规则依据

        // * 根据角色卡，真珠当前等级为 2 级（XP: 330），最大 HP 为 16。
        // * 角色升级必须符合经验值要求，且需要进行合法的升级步骤（更新等级、HP、法术位、已知法术等）。
        // * 虽然 DM 提供了升级操作，但未提供升级所需的具体选择（例如：除了已知法术，法术列表的变化等）。

        // ## 状态变更分析

        // * **被请求修改的数据**：经验值 (增加 1000)、等级 (升级)、已知法术 (学习“蛛网术”)、最大 HP (修改为 22)。
        // * **连带影响的数据**：熟练加值、法术位、Hit Dice、生命值骰点（规则要求 1d6+CON，需验证是否符合 22）、可选专长或特性（如 3 级解锁的子职业特性）。
        // * **缺失的数据**：升级后的具体等级数（当前 2 级，+1000 经验是否达到 3 级？）、法术替换选择（升级时可替换法术）、HP 提升的合理性验证（需基于职业骰点）。

        // ## 数据库操作

        // * **禁止更新**。
        // * **原因**：
        //     1. 经验值计算需核对升级表：当前 2 级 (330 XP)，+1000 XP = 1330 XP。根据 DND 5e 经验值升级表，3 级需要 900 XP，4 级需要 2700 XP，因此真珠应升级至 **3 级**，而非简单的“升级”。
        //     2. 升级至 3 级会获得新的子职业特性（需选择），以及新的法术位和已知法术上限（需确认具体学习与替换情况）。
        //     3. 未提供升级 HP 的骰点或固定值选择（从 16 变为 22，增加了 6 点，符合 1d6+2 的最大值，但需确认为固定值选择）。

        // ## 风险警告

        // * **信息不足**：DM 未说明真珠升至 3 级后，需要进行的子职业选择及法术替换细节。
        // * **规则冲突**：若直接强制修改为 22 HP，可能绕过了等级提升的计算流程。

        // ---

        // **请提供以下补充信息以完成升级：**
        // 1. 确认真珠升级至 **3 级**（基于 1330 XP）。
        // 2. 真珠作为术士（Sorcerer），升至 3 级需选择一个子职业（如果尚未选择，请指定），并确认是否替换旧法术。
        // 3. 确认 HP 从 16 提升至 22 的计算方式（例如：是否采取了该等级固定的最大值）。
        println!("Response: {response}");

        Ok(())
    }

    #[tokio::test]
    async fn test_new_summary() -> Result<(), Box<dyn std::error::Error>> {
        dotenvy::dotenv().ok();
        let (mut gemini_service, pool, _, _) = service_setup().await?;

        gemini_service.new_summary().await?;

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

        gemini_service.new_summary().await?;

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
        dotenvy::dotenv().ok();
        let (mut gemini_service, pool, _, _) = service_setup().await?;

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

        gemini_service.store_new_dialogue("", "", "").await?;

        let response: Vec<DialogueEntity> = sqlx::query_as("SELECT * FROM dialogues")
            .fetch_all(&pool)
            .await?;

        assert_eq!(response.len(), 0);

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue("当前在建卡步骤：选择职业。可回复：我选战士 / 我选游荡者 / 我选法师 / 我选牧师 / 我选游侠 / 我选圣武士。
如果要退出，回复：取消建卡。", "1483098634601107476", "dnd-5")
            .await?;

        let response: Vec<DialogueEntity> = sqlx::query_as("SELECT * FROM dialogues")
            .fetch_all(&pool)
            .await?;

        assert_eq!(response.len(), 0);

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                "当前在建卡步骤：选择职业，但如果你已经决定背景，可以直接说“我在酒馆喝酒”进入剧情。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                "系统提示：角色创建完成。
你发现自己站在一间昏暗的酒馆里，空气中弥漫着酒精味。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                "我可能会考虑去酒馆看看情况，但还没决定。",
                "1483098634601107489",
                "shaomo1",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                "如果我攻击哥布林会发生什么？我拔出剑冲上去。",
                "1483098634601107489",
                "shaomo1",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                "街角传来骚动，似乎有人在议论最近失踪的旅人……",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                "你现在可以自由探索城镇，酒馆就在前方。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                "三天后，你已经离开村庄，踏上前往北方的道路。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                "泽阿里尔正在整理装备，而庄芳宜已经走进酒馆开始喝酒。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
                "（系统正在初始化）酒馆里人声鼎沸，冒险者们在讨论最近的失踪事件。",
                "1483098634601107476",
                "dnd-5",
            )
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue("我决定先喝酒", "1483098634601107486", "anyTHING")
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue("喝酒。", "1483098634601107486", "anyTHING")
            .await?;

        sleep(Duration::from_secs(5));

        gemini_service
            .store_new_dialogue(
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

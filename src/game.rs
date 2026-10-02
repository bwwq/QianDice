use crate::dice;
use anyhow::{Context, Result, bail, ensure};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextInfo {
    pub platform: String,
    pub account: String,
    pub user: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub admin: bool,
}
impl ContextInfo {
    pub fn scope(&self) -> String {
        serde_json::to_string(&(&self.platform, &self.account)).unwrap()
    }
    pub fn room(&self) -> String {
        self.group
            .clone()
            .unwrap_or_else(|| format!("private:{}", self.user))
    }
    pub fn log_scope(&self) -> String {
        serde_json::to_string(&(&self.platform, &self.account, self.room())).unwrap()
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Card {
    pub name: String,
    pub rule: String,
    pub attrs: BTreeMap<String, i64>,
    pub growth: BTreeSet<String>,
    pub aliases: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Player {
    pub current: String,
    pub cards: BTreeMap<String, Card>,
    pub bindings: BTreeMap<String, String>,
    pub nickname: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Room {
    pub enabled: bool,
    pub rule: String,
    pub coc_rule: u8,
    pub log: String,
    pub recording: bool,
    pub last_log: String,
    pub initiative: BTreeMap<String, i64>,
    pub deck_used: BTreeMap<String, Vec<usize>>,
}
impl Default for Room {
    fn default() -> Self {
        Self {
            enabled: true,
            rule: "coc7".into(),
            coc_rule: 0,
            log: String::new(),
            recording: false,
            last_log: String::new(),
            initiative: BTreeMap::new(),
            deck_used: BTreeMap::new(),
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct World {
    pub players: BTreeMap<String, Player>,
    pub rooms: BTreeMap<String, Room>,
    pub replies: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRequest {
    pub context: ContextInfo,
    pub command: String,
    pub world: World,
    #[serde(default)]
    pub decks: BTreeMap<String, Deck>,
    #[serde(default)]
    pub rules: BTreeMap<String, RuleSpec>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSpec {
    pub id: String,
    pub label: String,
    pub faces: u32,
    #[serde(default = "comparison")]
    pub comparison: String,
    #[serde(default)]
    pub critical: Option<u32>,
    #[serde(default)]
    pub fumble: Option<u32>,
}
fn comparison() -> String {
    "lte".into()
}
impl RuleSpec {
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.id.is_empty() && self.id.len() < 80, "规则标识无效");
        ensure!((2..=1_000_000).contains(&self.faces), "规则骰面无效");
        ensure!(
            ["lte", "gte"].contains(&self.comparison.as_str()),
            "comparison须为lte或gte"
        );
        for n in [self.critical, self.fumble].into_iter().flatten() {
            ensure!(n >= 1 && n <= self.faces, "特殊骰点超出范围");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommandResult {
    pub public: String,
    pub private: Option<String>,
    pub world: World,
    #[serde(default)]
    pub export_log: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deck {
    #[serde(default)]
    pub without_replacement: bool,
    pub entries: Vec<DeckEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckEntry {
    pub text: String,
    #[serde(default = "one")]
    pub weight: u32,
}
fn one() -> u32 {
    1
}
pub fn built_in_decks() -> BTreeMap<String, Deck> {
    [
        (
            "临时疯狂",
            vec![
                "失忆：忘记近期发生的事件。",
                "身心症状：出现短暂的身体症状。",
                "暴力冲动：由守秘人决定表现。",
                "偏执：暂时无法信任周围的人。",
                "重要之人：把眼前的人认作重要之人。",
                "昏厥：短暂失去意识。",
                "逃跑：强烈希望离开现场。",
                "歇斯底里：无法抑制情绪。",
                "恐惧：产生强烈恐惧。",
                "躁狂：被某种行为冲动支配。",
            ],
        ),
        (
            "总结疯狂",
            vec![
                "失忆",
                "被窃",
                "遍体鳞伤",
                "暴力行为",
                "极端信念",
                "重要之人",
                "被收容",
                "逃避",
                "恐惧症",
                "躁狂症",
            ],
        ),
    ]
    .into_iter()
    .map(|(name, items)| {
        (
            name.into(),
            Deck {
                without_replacement: false,
                entries: items
                    .into_iter()
                    .map(|s| DeckEntry {
                        text: s.into(),
                        weight: 1,
                    })
                    .collect(),
            },
        )
    })
    .collect()
}
pub fn skill(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "str" | "力量" => "力量",
        "con" | "体质" => "体质",
        "siz" | "体型" => "体型",
        "dex" | "敏捷" => "敏捷",
        "app" | "外貌" => "外貌",
        "int" | "智力" | "灵感" => "智力",
        "pow" | "意志" => "意志",
        "edu" | "教育" => "教育",
        "san" | "理智" | "理智值" => "理智",
        "luck" | "幸运" => "幸运",
        "hp" | "生命" => "生命",
        "mp" | "魔法" => "魔法",
        _ => name,
    }
    .to_string()
}
pub fn grade(roll: u32, value: i64, house: u8) -> u8 {
    if roll == 1 || (house == 1 && roll <= 5) {
        5
    } else if roll == 100 || (value < 50 && roll >= 96) {
        0
    } else if roll as i64 <= value / 5 {
        4
    } else if roll as i64 <= value / 2 {
        3
    } else if roll as i64 <= value {
        2
    } else {
        1
    }
}
pub fn grade_name(g: u8) -> &'static str {
    ["大失败", "失败", "成功", "困难成功", "极难成功", "大成功"][g as usize]
}
fn card<'a>(world: &'a mut World, c: &ContextInfo) -> Result<&'a mut Card> {
    let p = world.players.entry(c.user.clone()).or_default();
    let name = p.bindings.get(&c.room()).unwrap_or(&p.current).clone();
    p.cards
        .get_mut(&name)
        .context("请先用 .st new 名称 创建角色卡，并用 .st lock 绑定当前群")
}
fn attrs_parse(text: &str) -> Result<Vec<(String, i64, bool)>> {
    let mut out = vec![];
    for token in text.split_whitespace() {
        let cut = token.find(|c: char| c.is_ascii_digit() || c == '-' || c == '+');
        let (key, n) = if let Some((k, v)) = token.split_once('=') {
            (k, v)
        } else if let Some(i) = cut {
            (&token[..i], &token[i..])
        } else {
            bail!("属性格式：力量=60 敏捷50")
        };
        ensure!(!key.is_empty(), "属性名称不能为空");
        let relative = !token.contains('=') && (n.starts_with('+') || n.starts_with('-'));
        let n: i64 = n.parse()?;
        ensure!((-100000..=100000).contains(&n), "属性数值超出范围");
        out.push((skill(key), n, relative));
    }
    ensure!(!out.is_empty(), "需要属性名称和数值");
    Ok(out)
}
fn parse_skill(text: &str) -> Result<(String, Option<i64>)> {
    let text = text.trim();
    if let Some((a, b)) = text.rsplit_once(' ') {
        if let Ok(v) = b.parse() {
            return Ok((skill(a.trim()), Some(v)));
        }
    }
    if let Some(i) = text.find(|c: char| c.is_ascii_digit()) {
        if i > 0 {
            return Ok((skill(&text[..i]), Some(text[i..].parse()?)));
        }
    }
    ensure!(!text.is_empty(), "请指定技能");
    Ok((skill(text), None))
}
pub fn normalize_command(input: &str) -> String {
    for prefix in ["rah", "rab", "rap", "ra", "rh", "r", "st", "sc", "en"] {
        if let Some(rest) = input.strip_prefix(prefix) {
            if rest.is_empty() || rest.starts_with(char::is_whitespace) {
                return input.into();
            }
            if prefix == "rab" || prefix == "rap" {
                let n = rest.bytes().take_while(u8::is_ascii_digit).count();
                return format!("{}{} {}", prefix, &rest[..n], rest[n..].trim());
            }
            let next = rest.chars().next().unwrap();
            if !next.is_ascii()
                || next.is_ascii_digit()
                || (prefix == "r" && (next == 'd' || next == 'b' || next == 'p' || next == '('))
                || (prefix == "st"
                    && [
                        "str", "san", "dex", "con", "pow", "edu", "int", "app", "siz", "hp", "mp",
                    ]
                    .iter()
                    .any(|s| rest.starts_with(s)))
            {
                return format!("{prefix} {rest}");
            }
        }
    }
    input.into()
}
pub fn execute(mut req: CommandRequest) -> Result<CommandResult> {
    let text = req.command.trim();
    ensure!(text.len() <= 4096, "指令过长");
    let (cmd, args) = text.split_once(' ').unwrap_or((text, ""));
    let args = args.trim();
    let c = &req.context;
    let room_id = c.room();
    let room = req.world.rooms.entry(room_id.clone()).or_default().clone();
    let mut result = CommandResult::default();
    if !room.enabled && cmd != "bot" {
        result.world = req.world;
        return Ok(result);
    }
    let mut rng = rand::thread_rng();
    result.public=match cmd{
        "help"|"帮助"=>"千变 · 跑团指令\n.r 1d100 / .rh 1d100 / .r 3#1d6\n.st new 名称 / .st 力量60 理智60 / .st lock\n.ra 侦查 / .rab1 侦查 / .rav 侦查 @对方\n.sc 0/1d6 / .en 侦查 / .ti / .li\n.coc / .dnd / .ri 名称 / .init\n.draw 牌堆 / .log on 名称 / .log end\n.setcoc 0 / .setrule coc7|dnd5e / .bot on|off\n.st help 查看角色卡操作".into(),
        "bot"=>{ensure!(c.admin,"仅骰主或群管理员可以启停");let enabled=match args{"on"=>true,"off"=>false,_=>bail!("用法：.bot on 或 .bot off")};req.world.rooms.get_mut(&room_id).unwrap().enabled=enabled;format!("千变已{}",if enabled{"启用"}else{"停用"})},
        "nn"=>{ensure!(args.len()<=100,"昵称过长");req.world.players.entry(c.user.clone()).or_default().nickname=args.into();format!("称呼已设为 {args}")},
        "setcoc"=>{ensure!(c.admin,"需要管理权限");let n:u8=args.parse()?;ensure!(n<=1,"内置房规：0=CoC7，1=1—5大成功；其他规则可用插件添加");req.world.rooms.get_mut(&room_id).unwrap().coc_rule=n;format!("CoC 房规设为 {n}")},
        "setrule"|"settemp"=>{ensure!(c.admin,"需要管理权限");ensure!(!args.is_empty()&&args.len()<80,"请提供规则标识");req.world.rooms.get_mut(&room_id).unwrap().rule=args.into();format!("本群规则设为 {args}，已有角色属性保持不变")},
        "st"=>{
            let (sub,tail)=args.split_once(' ').unwrap_or((args,""));let tail=tail.trim();
            match sub{
                "help"=>".st new 名称 / list / set 名称 / del 名称 / clear\n.st lock / unlock / temp coc7|dnd5e\n.st 力量60 理智60 / .st 理智 / .st 理智-5\n.st alias 自定义名称=已有属性\n.st export / .st import JSON".into(),
                "new"=>{ensure!(!tail.is_empty()&&tail.len()<=100,"角色名不能为空或过长");let p=req.world.players.entry(c.user.clone()).or_default();ensure!(!p.cards.contains_key(tail),"角色已存在");p.cards.insert(tail.into(),Card{name:tail.into(),rule:room.rule.clone(),..Default::default()});p.current=tail.into();format!("已创建并选择 {tail}")},
                "list"=>{let p=req.world.players.entry(c.user.clone()).or_default();p.cards.keys().map(|n|format!("{} {n}",if p.current==*n{"●"}else{"○"})).collect::<Vec<_>>().join("\n")},
                "set"=>{let p=req.world.players.entry(c.user.clone()).or_default();ensure!(p.cards.contains_key(tail),"找不到角色");p.current=tail.into();format!("默认角色：{tail}；群绑定保持不变")},
                "del"=>{let p=req.world.players.entry(c.user.clone()).or_default();ensure!(p.cards.remove(tail).is_some(),"找不到角色");p.bindings.retain(|_,v|v!=tail);if p.current==tail{p.current=String::new();}format!("已删除 {tail}")},
                "lock"=>{let p=req.world.players.entry(c.user.clone()).or_default();let target=if tail.is_empty(){p.current.clone()}else{tail.into()};ensure!(p.cards.contains_key(&target),"角色不存在");p.bindings.insert(room_id.clone(),target.clone());format!("当前会话绑定：{target}")},
                "unlock"=>{req.world.players.entry(c.user.clone()).or_default().bindings.remove(&room_id);"已解除当前会话绑定".into()},
                "clear"=>{card(&mut req.world,c)?.attrs.clear();"已清空当前角色属性".into()},
                "temp"=>{ensure!(!tail.is_empty(),"需要规则名称");card(&mut req.world,c)?.rule=tail.into();format!("角色模板：{tail}；属性保持不变")},
                "alias"=>{let(a,b)=tail.split_once('=').context("格式：.st alias 别名=属性")?;let ca=card(&mut req.world,c)?;let key=skill(b);ensure!(ca.attrs.contains_key(&key),"目标属性不存在");ca.aliases.insert(a.trim().into(),key);"别名已保存".into()},
                "export"=>serde_json::to_string(card(&mut req.world,c)?)?,
                "import"=>{let ca:Card=serde_json::from_str(tail)?;ensure!(!ca.name.is_empty()&&ca.name.len()<=100&&ca.attrs.len()<=500,"角色数据无效");let p=req.world.players.entry(c.user.clone()).or_default();ensure!(!p.cards.contains_key(&ca.name),"同名角色已存在，请先更名");p.current=ca.name.clone();p.cards.insert(ca.name.clone(),ca);"角色已导入".into()},
                _=>{
                    let ca=card(&mut req.world,c)?;
                    if args.is_empty(){format!("{} [{}]\n{}",ca.name,ca.rule,ca.attrs.iter().map(|(k,v)|format!("{k}：{v}")).collect::<Vec<_>>().join("  "))}
                    else if let Some(value)=ca.attrs.get(ca.aliases.get(args).unwrap_or(&skill(args))){format!("{args}：{value}")}
                    else {let values=attrs_parse(args)?;for (key,n,relative) in &values{let value=if *relative{ca.attrs.get(key).copied().unwrap_or(0).checked_add(*n).context("属性修改溢出")?}else{*n};ensure!((-100000..=100000).contains(&value),"属性超出范围");ca.attrs.insert(key.clone(),value);}values.iter().map(|(k,_,_)|format!("{k}：{}",ca.attrs[k])).collect::<Vec<_>>().join("  ")}
                }
            }
        },
        "coc"=>{let mut a=BTreeMap::new();for name in ["力量","体质","敏捷","外貌","意志","幸运"]{a.insert(name.to_string(),dice::roll("3d6*5",100)?.total);}for name in ["体型","智力","教育"]{a.insert(name.to_string(),dice::roll("(2d6+6)*5",100)?.total);}a.insert("理智".into(),a["意志"]);a.insert("魔法".into(),a["意志"]/5);a.insert("生命".into(),(a["体质"]+a["体型"])/10);format!("CoC7 人物属性（用 .st 录入）\n{}",a.iter().map(|(k,v)|format!("{k}{v}")).collect::<Vec<_>>().join(" "))},
        "dnd"=>{let values=(0..6).map(|_|dice::roll("4d6k3",20).map(|v|v.total)).collect::<Result<Vec<_>>>()?;format!("DND5E 属性（4d6取高3）：{values:?}")},
        "sc"=>{let mut parts=args.split_whitespace();let loss=parts.next().context("格式：.sc 0/1d6 [SAN]")?;let (success,failure)=loss.split_once('/').context("需要成功/失败损失")?;let explicit=parts.next().map(str::parse::<i64>).transpose()?;let san=if let Some(v)=explicit{v}else{*card(&mut req.world,c)?.attrs.get("理智").context("角色未录入理智")?};ensure!((0..=99).contains(&san),"理智数值无效");let value=rng.gen_range(1..=100);let chosen=if value as i64<=san{success}else{failure};let lost=if grade(value,san,room.coc_rule)==0{dice::max_value(chosen)?}else{dice::roll(chosen,100)?.total};ensure!(lost>=0,"损失不能为负数");let remaining=(san-lost).max(0);if explicit.is_none(){card(&mut req.world,c)?.attrs.insert("理智".into(),remaining);}format!("理智检定 {value}/{san}：{}，损失 {lost}，剩余 {remaining}{}",if value as i64<=san{"成功"}else{"失败"},if lost>=5{"；单次损失≥5，请进行智力检定判断临时疯狂"}else{""})},
        "en"=>{let input=if args.is_empty(){card(&mut req.world,c)?.growth.clone().into_iter().collect::<Vec<_>>()}else{args.split(',').map(str::to_string).collect()};ensure!(input.len()<=100,"成长项目过多");let mut output=vec![];for s in input{let (name,explicit)=parse_skill(&s)?;let value=if let Some(v)=explicit{v}else{*card(&mut req.world,c)?.attrs.get(&name).context("技能不存在")?};let roll=rng.gen_range(1..=100);let add=if roll>value||roll>=96{rng.gen_range(1..=10)}else{0};if explicit.is_none(){let ca=card(&mut req.world,c)?;ca.attrs.insert(name.clone(),value+add);ca.growth.remove(&name);}output.push(format!("{name}：{roll}/{value} → +{add} = {}",value+add));}output.join("\n")},
        "ti"|"li"=>{let name=if cmd=="ti"{"临时疯狂"}else{"总结疯狂"};draw(name,&req.decks,req.world.rooms.get_mut(&room_id).unwrap(),0,&mut rng)?},
        "draw"|"drawh"=>{let out=draw(args,&req.decks,req.world.rooms.get_mut(&room_id).unwrap(),0,&mut rng)?;if cmd=="drawh"{result.private=Some(out);"进行了暗抽。".into()}else{out}},
        "ri"=>{let mut parts=args.split_whitespace();let name=parts.next().filter(|s|!s.is_empty()).unwrap_or(&c.user);let modifier=parts.next().unwrap_or("0").parse::<i64>()?;let n=dice::roll("1d20",20)?.total.checked_add(modifier).context("先攻数值溢出")?;req.world.rooms.get_mut(&room_id).unwrap().initiative.insert(name.into(),n);format!("{name} 先攻：{n}")},
        "init"=>{let rr=req.world.rooms.get_mut(&room_id).unwrap();if args=="clear"{ensure!(c.admin,"需要管理权限");rr.initiative.clear();"先攻已清空".into()}else{let mut v:Vec<_>=rr.initiative.iter().collect();v.sort_by(|a,b|b.1.cmp(a.1));v.iter().enumerate().map(|(i,(n,v))|format!("{}. {n}：{v}",i+1)).collect::<Vec<_>>().join("\n")}},
        "log"=>{let (sub,name)=args.split_once(' ').unwrap_or((args,""));let rr=req.world.rooms.get_mut(&room_id).unwrap();match sub{
            "on"=>{if rr.log.is_empty(){rr.log=if name.is_empty(){format!("团录-{}",chrono::Utc::now().format("%Y%m%d-%H%M%S"))}else{name.into()};}rr.recording=true;format!("开始记录：{}",rr.log)},
            "off"=>{rr.recording=false;"记录已暂停，可用 .log on 继续".into()},
            "end"=>{ensure!(!rr.log.is_empty(),"没有进行中的日志");result.export_log=Some(rr.log.clone());let name=std::mem::take(&mut rr.log);rr.last_log=name.clone();rr.recording=false;format!("记录已结束：{name}")},
            "stop"=>{rr.recording=false;rr.log.clear();"已停止记录，已有内容仍保留".into()},
            "upload"=>{let name=if !name.is_empty(){name.to_string()}else if !rr.log.is_empty(){rr.log.clone()}else{rr.last_log.clone()};ensure!(!name.is_empty(),"请提供日志名称");result.export_log=Some(name.clone());format!("准备团录：{name}")},
            "info"=>format!("{}：{}",if rr.recording{"记录中"}else{"已暂停"},rr.log),
            _=>bail!(".log on [名称] / off / end / stop / info / upload [名称]")}},
        "reply"=>{ensure!(c.admin,"需要管理权限");let(k,v)=args.split_once('=').context("用法：.reply 指令=回复；空回复表示删除")?;ensure!(!k.is_empty()&&k.len()<80,"指令名称无效");if v.is_empty(){req.world.replies.remove(k);}else{req.world.replies.insert(k.into(),v.into());}"自定义回复已保存".into()},
        "rav"=>{let parts:Vec<_>=args.split_whitespace().collect();ensure!(parts.len()>=2,"用法：.rav 技能 @用户ID（双方读取当前群角色）");let name=skill(parts[0]);let other=parts.last().unwrap().trim_start_matches('@');let v1=*card(&mut req.world,c)?.attrs.get(&name).context("己方技能不存在")?;ensure!(req.world.players.get(other).is_some_and(|p|p.bindings.contains_key(&room_id)),"对方需要先在当前群绑定角色");let mut oc=c.clone();oc.user=other.into();let v2=*card(&mut req.world,&oc)?.attrs.get(&name).context("对方技能不存在")?;let r1=rng.gen_range(1..=100);let r2=rng.gen_range(1..=100);let g1=grade(r1,v1,room.coc_rule);let g2=grade(r2,v2,room.coc_rule);let cmp=(g1,v1).cmp(&(g2,v2));format!("{} {name} {r1}/{v1} {}\n{other} {name} {r2}/{v2} {}\n{}",c.user,grade_name(g1),grade_name(g2),match cmp{std::cmp::Ordering::Greater=>"己方胜出",std::cmp::Ordering::Less=>"对方胜出",_=>"平局"})},
        _ if cmd.starts_with("ra")=>{
            let mut modifier=cmd.trim_start_matches("ra");let hidden=modifier.starts_with('h');if hidden{modifier=&modifier[1..];}
            let penalty=modifier.starts_with('p');let extra=if modifier.starts_with('b')||penalty{modifier[1..].parse::<u32>().unwrap_or(1)}else{0};ensure!(extra<=10,"奖惩骰最多10个");
            let(times,spec)=if let Some((n,s))=args.split_once('#'){(n.parse::<usize>()?,s)}else{(1,args)};ensure!((1..=20).contains(&times),"重复次数应为1到20");
            let(name,explicit)=parse_skill(spec)?;let value=if let Some(v)=explicit{v}else{let ca=card(&mut req.world,c)?;let key=ca.aliases.get(&name).unwrap_or(&name);*ca.attrs.get(key).context("技能未录入，请在指令中提供数值")?};
            let rule_id=card(&mut req.world,c).ok().map(|ca|ca.rule.clone()).filter(|s|!s.is_empty()).unwrap_or_else(||room.rule.clone());
            let mut lines=vec![];for _ in 0..times{
                if let Some(rule)=req.rules.get(&rule_id){rule.validate()?;ensure!(extra==0,"此自定义规则不使用CoC奖惩骰");let v=rng.gen_range(1..=rule.faces);let passed=if rule.comparison=="gte"{v as i64>=value}else{v as i64<=value};let verdict=if rule.critical==Some(v){"大成功"}else if rule.fumble==Some(v){"大失败"}else if passed{"成功"}else{"失败"};lines.push(format!("{} · {name} {v}/{value}：{verdict}",rule.label));}
                else{let(v,d)=dice::percentile(extra,penalty,&mut rng);let g=grade(v,value,room.coc_rule);if explicit.is_none()&&g>=2{card(&mut req.world,c)?.growth.insert(name.clone());}lines.push(format!("{name} {v}/{value}：{}{}",grade_name(g),if extra>0{format!("（{d}）")}else{String::new()}));}
            }let out=lines.join("\n");if hidden{result.private=Some(out);"进行了暗检定。".into()}else{out}
        },
        _ if cmd=="r"||cmd=="rh"||cmd.starts_with("r#")||cmd.starts_with("r3")=>{
            let hidden=cmd=="rh";let expr=args.split_once(' ').map(|(a,_)|a).unwrap_or(args);let reason=args.strip_prefix(expr).unwrap_or("").trim();let(times,expr)=if let Some((n,e))=expr.split_once('#'){(n.parse::<usize>()?,e)}else{(1,expr)};ensure!((1..=20).contains(&times),"重复次数应为1到20");let faces=if room.rule=="dnd5e"{20}else{100};let mut lines=vec![];for _ in 0..times{let r=dice::roll(expr,faces)?;lines.push(format!("{} = {}{}",r.expression,r.total,if r.detail.is_empty(){String::new()}else{format!(" [{}]",r.detail.join("；"))}));}let out=format!("{}{}",if reason.is_empty(){String::new()}else{format!("{reason}\n")},lines.join("\n"));if hidden{result.private=Some(out);"进行了暗骰。".into()}else{out}
        },
        "adv"|"dis"=>{let modifier=if args.is_empty(){0}else{args.parse::<i64>()?};let r=dice::roll(if cmd=="adv"{"2d20kh1"}else{"2d20kl1"},20)?;let total=r.total.checked_add(modifier).context("加值溢出")?;format!("{} + {modifier} = {} ({})",r.total,total,r.detail.join("；"))},
        _=>req.world.replies.get(cmd).cloned().unwrap_or_else(||"未知指令，使用 .help 查看帮助".into()),
    };
    ensure!(
        result.public.len() <= 24000,
        "结果过长，请在管理端查看或减少投掷数量"
    );
    result.world = req.world;
    Ok(result)
}
fn draw(
    name: &str,
    decks: &BTreeMap<String, Deck>,
    room: &mut Room,
    depth: usize,
    rng: &mut impl Rng,
) -> Result<String> {
    ensure!(depth < 8, "牌堆引用过深或循环");
    let deck = decks.get(name).context("牌堆不存在")?;
    ensure!(
        !deck.entries.is_empty() && deck.entries.len() <= 10000,
        "牌堆为空或过大"
    );
    let used = room.deck_used.entry(name.into()).or_default();
    let available: Vec<_> = deck
        .entries
        .iter()
        .enumerate()
        .filter(|(i, e)| e.weight > 0 && (!deck.without_replacement || !used.contains(i)))
        .collect();
    ensure!(!available.is_empty(), "牌堆已抽完，请在管理端重置");
    let total: u64 = available.iter().map(|(_, e)| e.weight as u64).sum();
    let mut pick = rng.gen_range(0..total);
    let mut selected = available[0];
    for item in available {
        if pick < item.1.weight as u64 {
            selected = item;
            break;
        }
        pick -= item.1.weight as u64;
    }
    let index = selected.0;
    let text = selected.1.text.clone();
    if deck.without_replacement {
        used.push(index);
    }
    let mut output = String::new();
    let mut remain = text.as_str();
    while let Some(i) = remain.find('{') {
        output.push_str(&remain[..i]);
        let rest = &remain[i + 1..];
        let (j, target) = rest
            .find('}')
            .map(|j| (j, &rest[..j]))
            .context("牌堆引用缺少右括号")?;
        output.push_str(&draw(target, decks, room, depth + 1, rng)?);
        remain = &rest[j + 1..];
        ensure!(output.len() < 12000, "牌堆结果过长");
    }
    output.push_str(remain);
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coc_boundaries() {
        assert_eq!(grade(1, 60, 0), 5);
        assert_eq!(grade(12, 60, 0), 4);
        assert_eq!(grade(30, 60, 0), 3);
        assert_eq!(grade(60, 60, 0), 2);
        assert_eq!(grade(61, 60, 0), 1);
        assert_eq!(grade(96, 49, 0), 0);
        assert_eq!(grade(96, 50, 0), 1);
        assert_eq!(grade(100, 99, 0), 0);
    }
    #[test]
    fn group_binding_and_explicit_san() {
        let c = ContextInfo {
            platform: "qq".into(),
            account: "a".into(),
            user: "u".into(),
            group: Some("g".into()),
            name: "u".into(),
            admin: false,
        };
        let mut w = World::default();
        for cmd in [
            "st new 调查员",
            "st 理智60",
            "st new 另一张",
            "st 理智80",
            "st set 调查员",
            "st lock",
            "st set 另一张",
            "sc 0/0 20",
        ] {
            w = execute(CommandRequest {
                context: c.clone(),
                command: cmd.into(),
                world: w,
                decks: built_in_decks(),
                rules: BTreeMap::new(),
            })
            .unwrap()
            .world;
        }
        assert_eq!(card(&mut w, &c).unwrap().name, "调查员");
        assert_eq!(card(&mut w, &c).unwrap().attrs["理智"], 60);
    }
}

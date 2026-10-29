mod numeric;
mod conversion;
use openvector::saved_profiles::{RecentBasicProfiles, SavedAdvancedProfiles};
use serde_json::{json, Value};
use std::{collections::HashMap, io::{Read, Write}};

type Result<T, E = String> = std::result::Result<T, E>;
struct Arguments { values: HashMap<String, String>, positionals: Vec<String> }
impl Arguments {
    fn parse(args: &[String], strings: &[&str], booleans: &[&str]) -> Result<Self> {
        let mut result = Self { values: HashMap::new(), positionals: Vec::new() };
        let mut arguments = args.iter();
        while let Some(argument) = arguments.next() {
            if argument == "--" { result.positionals.extend(arguments.cloned()); break; }
            if argument == "-" || !argument.starts_with('-') { result.positionals.push(argument.clone()); continue; }
            let argument = match argument.as_str() { "-o" => "--output", "-f" => "--force", _ => argument };
            let (key, inline) = argument.strip_prefix("--").ok_or_else(|| format!("Unknown option '{argument}'"))?.split_once('=').map_or((argument.trim_start_matches("--"), None), |(k,v)| (k, Some(v)));
            let value = if booleans.contains(&key) {
                if inline.is_some() { return Err(format!("Option '--{key}' does not take an argument")); }
                "true".to_owned()
            } else if strings.contains(&key) {
                inline.map(str::to_owned).or_else(|| arguments.next().cloned()).ok_or_else(|| format!("Option '--{key} <value>' argument missing"))?
            } else { return Err(format!("Unknown option '--{key}'")); };
            result.values.insert(key.into(), value);
        }
        Ok(result)
    }
    fn get(&self, name: &str) -> Option<&str> { self.values.get(name).map(String::as_str) }
    fn flag(&self, name: &str) -> bool { self.values.contains_key(name) }
}
fn read_bytes(filename: &str) -> Result<Vec<u8>> {
    if filename == "-" { let mut bytes = Vec::new(); std::io::stdin().read_to_end(&mut bytes).map_err(|e|e.to_string())?; Ok(bytes) }
    else { std::fs::read(filename).map_err(|e|e.to_string()) }
}
fn read_json(filename: &str) -> Result<Value> { serde_json::from_slice(&read_bytes(filename)?).map_err(|e|e.to_string()) }
fn print_json(value: &Value) -> Result<()> {
    let stdout = std::io::stdout(); let mut out = stdout.lock();
    serde_json::to_writer_pretty(&mut out, value).map_err(|e|e.to_string())?;
    out.write_all(b"\n").map_err(|e|e.to_string())
}
fn profiles(args: &[String]) -> Result<()> {
    let args = Arguments::parse(args, &["file","name","input","new-name"], &["force"])?;
    let command = args.positionals.first().map(String::as_str).unwrap_or("");
    if args.positionals.len()!=1 || !["list","show","save","select","rename","delete"].contains(&command) {
        return Err("Choose profiles list, show, save, select, rename or delete. Use profiles --help for syntax.".into());
    }
    let file = args.get("file").filter(|v|!v.is_empty() && *v!="-").ok_or("--file must name a profile storage file.")?;
    if (command=="list") == args.get("name").is_some() { return Err(if command=="list" {"profiles list does not accept --name."} else {"--name is required."}.into()); }
    if command!="save" && args.get("input").is_some() {return Err("--input is only valid with profiles save.".into());}
    if !["save","rename"].contains(&command) && args.flag("force") {return Err("--force is only valid with profiles save or rename.".into());}
    if (command=="rename") != args.get("new-name").is_some() {return Err(if command=="rename" {"profiles rename requires --new-name."}else{"--new-name is only valid with profiles rename."}.into());}
    if command=="save" && args.get("input").is_none_or(str::is_empty) {return Err("profiles save requires --input with a JSON path or - for stdin.".into());}
    let mut list = SavedAdvancedProfiles::read_file(file)?;
    let entries = list.snapshot();
    let name = args.get("name").unwrap_or("");
    let missing = || format!("Profile not found: {name}");
    let output = match command {
        "list" => Value::Array(entries.iter().map(|e|json!({"name":e.name,"profile":e.profile.to_json()})).collect()),
        "show" => entries.iter().find(|e|e.name==name).ok_or_else(missing)?.profile.to_json(),
        "save" => {
            if !args.flag("force") && entries.iter().any(|e|e.name==name) {return Err("Profile already exists. Use --force to replace it.".into());}
            list.put_json(name,&read_json(args.get("input").unwrap())?)?;
            let entry=list.snapshot().remove(0);list.write_file(file)?;
            json!({"name":entry.name,"profile":entry.profile.to_json()})
        },
        "rename" => {
            let renamed=list.rename(name,args.get("new-name").unwrap(),args.flag("force"))?;
            if renamed.index<0{return Err(missing());} if renamed.changed{list.write_file(file)?;}
            json!({"index":renamed.index,"changed":renamed.changed,"replacedIndex":renamed.replaced_index})
        },
        "delete" => {let index=list.remove(name);if index<0{return Err(missing());}list.write_file(file)?;json!({"index":index})},
        _ => {let (index,profile)=list.select(name);let profile=profile.ok_or_else(missing)?;list.write_file(file)?;json!({"index":index,"profile":profile.to_json()})},
    };
    print_json(&output)
}
/// Zero-based recent entry index. Only ASCII digits are accepted, so signs,
/// spaces and fractions fail instead of being normalized.
fn recent_index(value: &str, option: &str) -> Result<usize> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) { return Err(format!("{option} must be a zero-based integer.")); }
    value.parse().map_err(|_| format!("{option} must be a zero-based integer."))
}
/// Inspects or clears recent basic settings through the shared Rust collection.
fn recent_settings(args: &[String]) -> Result<()> {
    let args = Arguments::parse(args, &["file","index"], &[])?;
    let command = args.positionals.first().map(String::as_str).unwrap_or("");
    if args.positionals.len()!=1 || !["list","show","clear"].contains(&command) {
        return Err("Choose recent-settings list, show or clear. Use recent-settings --help for syntax.".into());
    }
    let file = args.get("file").filter(|v|!v.is_empty() && *v!="-").ok_or("--file must name a recent settings storage file.")?;
    if (command=="show") != args.get("index").is_some() { return Err(if command=="show" {"recent-settings show requires --index."} else {"--index is only valid with recent-settings show."}.into()); }
    let index = args.get("index").map(|value| recent_index(value, "--index")).transpose()?;
    let mut list = RecentBasicProfiles::read_file(file)?;
    let output = match command {
        // The hidden fifth entry stays listed: it is persisted and still matches its label.
        "list" => { let visible = list.visible_entries().len(); Value::Array(list.entries().iter().enumerate().map(|(i,e)| json!({"label":e.label,"visible":(i<visible),"profile":e.profile.to_json()})).collect()) },
        "show" => { let index = index.unwrap(); list.entries().get(index).ok_or_else(|| format!("Recent settings entry not found: {index}"))?.profile.to_json() },
        // Clearing an empty list leaves storage untouched, including a missing file.
        _ => { let removed = list.entries().len(); if removed > 0 { list.clear(); list.write_file(file)?; } json!({"removed":removed}) },
    };
    print_json(&output)
}
fn run(args: &[String]) -> Result<i32> {
    let command=args.first().map(String::as_str).unwrap_or("");
    if command.is_empty() || command=="help" || args.iter().any(|a|a=="--help" || a=="-h") {
        let subject=if command=="help" {args.get(1).map(String::as_str).unwrap_or("")}else{command};
        let help=match subject {"vectorize"=>include_str!("help-vectorize.txt"),"batch"=>include_str!("help-batch.txt"),"profiles"=>include_str!("help-profiles.txt"),"recent-settings"=>include_str!("help-recent-settings.txt"),_=>include_str!("help.txt")};
        print!("{help}");return Ok(0);
    }
    if command=="--version" && args.len()==1 {println!("{}",env!("CARGO_PKG_VERSION"));return Ok(0);}
    match command {"profiles"=>{profiles(&args[1..])?;Ok(0)},"recent-settings"=>{recent_settings(&args[1..])?;Ok(0)},"vectorize"=>{conversion::vectorize(&args[1..])?;Ok(0)},"batch"=>conversion::batch(&args[1..]),_=>{numeric::run(command,&args[1..])?;Ok(0)}}
}
fn main() {
    let args=std::env::args().skip(1).collect::<Vec<_>>();
    match run(&args) {Ok(code)=>std::process::exit(code),Err(error)=>{eprintln!("{error}");std::process::exit(1)}}
}

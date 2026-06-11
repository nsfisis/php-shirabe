/// Service tag to autoconfigure commands.
///
/// PHP attribute: #[\Attribute(\Attribute::TARGET_CLASS)]
#[derive(Debug)]
pub struct AsCommand {
    pub name: String,
    pub description: Option<String>,
}

impl AsCommand {
    pub fn new(
        name: String,
        description: Option<String>,
        aliases: Vec<String>,
        hidden: bool,
    ) -> Self {
        let mut this = Self { name, description };

        if !hidden && aliases.is_empty() {
            return this;
        }

        let mut name: Vec<String> = this.name.split('|').map(|s| s.to_string()).collect();
        name.extend(aliases);

        if hidden && "" != name[0] {
            name.insert(0, String::new());
        }

        this.name = name.join("|");

        this
    }
}

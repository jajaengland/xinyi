//! Kernel-owned launch profiles. Restricted profiles fail closed for new calls.
pub const FILE_READ: u32 = 1;
pub const FILE_WRITE: u32 = 2;
pub const LAUNCH: u32 = 4;
pub const SHARED: u32 = 8;
pub const SHELL: u32 = 16;
pub const AUDIO: u32 = 32;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Trusted,
    ReadOnly,
    Media,
    Isolated,
}
impl Profile {
    pub fn mask(self) -> u32 {
        match self {
            Self::Trusted => FILE_READ | FILE_WRITE | LAUNCH | SHARED | SHELL | AUDIO,
            Self::Media => FILE_READ | AUDIO,
            Self::ReadOnly => FILE_READ,
            Self::Isolated => 0,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Media => "media",
            Self::Trusted => "trusted",
            Self::ReadOnly => "read-only",
            Self::Isolated => "isolated",
        }
    }
    pub fn restrict(self, other: Self) -> Self {
        if self == Self::Isolated || other == Self::Isolated {
            Self::Isolated
        } else if self == Self::ReadOnly || other == Self::ReadOnly {
            Self::ReadOnly
        } else if self == Self::Media || other == Self::Media {
            Self::Media
        } else {
            Self::Trusted
        }
    }
    pub fn allows(self, number: u32, first: u32) -> bool {
        if self == Self::Trusted {
            return true;
        }
        match number {
            // Own process, window, console, memory, time and policy query.
            1 | 4 | 20 | 50..=56 | 59..=66 | 70 | 85..=87 | 95 => true,
            57 | 78 | 79 | 82 | 88 | 90 | 91 | 93 => matches!(self, Self::ReadOnly | Self::Media),
            103 => matches!(self,Self::ReadOnly|Self::Media)&&matches!(first,2|3|4),
            96 => first == 0,
            97..=99 => self == Self::Media,
            100 => self == Self::Media && first == 0,
            // Reading colours is harmless; applying/saving changes global state.
            84 => first == 0,
            _ => false,
        }
    }
    /// This rule only REMOVES authority; filenames never grant privileges.
    pub fn for_name(name: &str) -> Self {
        let leaf = name
            .rsplit(|c| c == '/' || c == '\\')
            .next()
            .unwrap_or(name);
        if leaf.eq_ignore_ascii_case("player") || leaf.eq_ignore_ascii_case("player.elf") {
            Self::Media
        } else if leaf.eq_ignore_ascii_case("viewer") || leaf.eq_ignore_ascii_case("viewer.bin") {
            Self::ReadOnly
        } else if leaf.eq_ignore_ascii_case("bigdemo") || leaf.eq_ignore_ascii_case("bigdemo.elf") {
            Self::Isolated
        } else {
            Self::Trusted
        }
    }
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Media => "--media ",
            Self::Trusted => "",
            Self::ReadOnly => "--read-only ",
            Self::Isolated => "--isolated ",
        }
    }
    pub fn title(self, title: &mut [u8; 48], length: usize) -> usize {
        let suffix: &[u8] = match self {
            Self::Media => b" [media]",
            Self::Trusted => b"",
            Self::ReadOnly => b" [RO]",
            Self::Isolated => b" [isolated]",
        };
        let n = length.min(title.len() - suffix.len());
        title[n..n + suffix.len()].copy_from_slice(suffix);
        n + suffix.len()
    }
}
pub struct Launch<'a> {
    pub name: &'a str,
    pub arguments: &'a str,
    pub profile: Profile,
}
fn split(s: &str) -> (&str, &str) {
    s.split_once(char::is_whitespace).unwrap_or((s, ""))
}
pub fn parse(command: &str) -> Result<Launch<'_>, &'static str> {
    let command = command.trim();
    // Keep the returned slices tied to command rather than temporary strings.
    let (first, rest) = split(command);
    let (profile, remaining) = match first {
        "--media" => (Profile::Media, rest.trim()),
        "--read-only" => (Profile::ReadOnly, rest.trim()),
        "--isolated" => (Profile::Isolated, rest.trim()),
        _ if first.starts_with("--") => return Err("Unknown launch mode"),
        _ => (Profile::Trusted, command),
    };
    let (name, arguments) = split(remaining);
    if name.is_empty() || name.starts_with("--") || name.len() > 105 || arguments.trim().len() > 128
    {
        return Err("Usage: run [--read-only|--isolated|--media] APP [ARGS]");
    }
    Ok(Launch {
        name,
        arguments: arguments.trim(),
        profile: profile.restrict(Profile::for_name(name)),
    })
}

use Version::*;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct QueryCommand {
    _v: Version,
    pub action: QueryAction,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Version {
    V0,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum QueryAction {
    Download(ActionDownload),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ActionDownload {
    pub target: DownloadTarget,
    pub format: DownloadFormat,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DownloadTarget {
    Single,
    All,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DownloadFormat {
    Png,
    Webp,
}

impl QueryCommand {
    pub fn download(target: DownloadTarget, format: DownloadFormat) -> Self {
        use {QueryAction::*, Version::*};

        Self {
            _v: V0,
            action: Download(ActionDownload { target, format }),
        }
    }

    pub fn encode(&self) -> String {
        let mut out = String::new();

        self._v.encode(&mut out);
        self.action.encode(self._v, &mut out);

        out
    }

    pub fn decode(mut data: &str) -> Option<Self> {
        let mut d = Decoder(&mut data);

        let _v = Version::decode(&mut d)?;
        let action = QueryAction::decode(_v, &mut d)?;

        Some(Self { _v, action })
    }
}

impl Version {
    fn encode(&self, out: &mut String) {
        match self {
            Self::V0 => out.push('0'),
        }
    }

    fn decode(d: &mut Decoder<'_>) -> Option<Self> {
        match d.eat()? {
            '0' => Some(Self::V0),
            _ => None,
        }
    }
}

impl QueryAction {
    fn encode(&self, v: Version, out: &mut String) {
        use Version::*;

        match v {
            V0 => match self {
                QueryAction::Download(action_download) => {
                    out.push('d');
                    action_download.encode(v, out)
                }
            },
        }
    }

    fn decode(v: Version, d: &mut Decoder<'_>) -> Option<Self> {
        match v {
            V0 => match d.eat()? {
                'd' => {
                    let action_download = ActionDownload::decode(v, d)?;

                    Some(Self::Download(action_download))
                }
                _ => None,
            },
        }
    }
}

impl ActionDownload {
    fn encode(&self, v: Version, out: &mut String) {
        match v {
            V0 => {
                let Self { target, format } = self;
                target.encode(v, out);
                format.encode(v, out);
            }
        }
    }

    fn decode(v: Version, d: &mut Decoder<'_>) -> Option<Self> {
        let target = DownloadTarget::decode(v, d)?;
        let format = DownloadFormat::decode(v, d)?;

        Some(Self { target, format })
    }
}

impl DownloadTarget {
    fn encode(&self, v: Version, out: &mut String) {
        match v {
            V0 => match self {
                Self::Single => out.push('s'),
                Self::All => out.push('a'),
            },
        }
    }

    fn decode(v: Version, d: &mut Decoder<'_>) -> Option<Self> {
        match v {
            V0 => match d.eat()? {
                's' => Some(Self::Single),
                'a' => Some(Self::All),
                _ => None,
            },
        }
    }
}

impl DownloadFormat {
    fn encode(&self, v: Version, out: &mut String) {
        match v {
            V0 => match self {
                Self::Png => out.push('p'),
                Self::Webp => out.push('w'),
            },
        }
    }

    fn decode(v: Version, d: &mut Decoder<'_>) -> Option<Self> {
        match v {
            V0 => match d.eat()? {
                'p' => Some(Self::Png),
                'w' => Some(Self::Webp),
                _ => None,
            },
        }
    }

    pub fn ext(&self) -> &'static str {
        match self {
            DownloadFormat::Png => "png",
            DownloadFormat::Webp => "webp",
        }
    }

    pub fn is_fine_for_sending_alone(&self) -> bool {
        !matches!(self, Self::Webp)
    }
}

struct Decoder<'a>(&'a str);

impl Decoder<'_> {
    fn eat(&mut self) -> Option<char> {
        let c = self.0.chars().next()?;

        self.0 = &self.0[c.len_utf8()..];

        Some(c)
    }
}

#[cfg(test)]
mod tests {
    use crate::query_command::QueryCommand;

    use super::{DownloadFormat, DownloadTarget};

    #[test]
    fn smoke() {
        let command = QueryCommand::download(DownloadTarget::Single, DownloadFormat::Png);

        assert_eq!(command.encode(), "0dsp");
        assert_eq!(QueryCommand::decode(&command.encode()).unwrap(), command);
    }
}

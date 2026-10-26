use crate::fat16::{display_name, short_name, Error, Result};
pub const MAX_DEPTH: usize = 8;
pub const MAX_PATH: usize = MAX_DEPTH * 13 + 1;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Path {
    pub names: [[u8; 11]; MAX_DEPTH],
    pub depth: usize,
}
impl Path {
    pub const ROOT: Self = Self {
        names: [[0; 11]; MAX_DEPTH],
        depth: 0,
    };
    pub fn parse(text: &str, base: &Self) -> Result<Self> {
        let mut path = if text.starts_with('/') || text.starts_with('\\') {
            Self::ROOT
        } else {
            *base
        };
        for part in text.split(['/', '\\']) {
            match part {
                "" | "." => (),
                ".." => {
                    if path.depth != 0 {
                        path.depth -= 1;
                        path.names[path.depth] = [0; 11];
                    }
                }
                _ => {
                    if path.depth == MAX_DEPTH {
                        return Err(Error::PathTooDeep);
                    }
                    path.names[path.depth] = short_name(part)?;
                    path.depth += 1;
                }
            }
        }
        Ok(path)
    }
    pub fn encode(&self, output: &mut [u8; MAX_PATH]) -> usize {
        output[0] = b'/';
        let mut length = 1;
        for index in 0..self.depth {
            if index != 0 {
                output[length] = b'/';
                length += 1;
            }
            let mut name = [0; 12];
            let count = display_name(&self.names[index], &mut name);
            output[length..length + count].copy_from_slice(&name[..count]);
            length += count;
        }
        length
    }
    pub fn contains(&self, child: &Self) -> bool {
        self.depth <= child.depth && self.names[..self.depth] == child.names[..self.depth]
    }
}

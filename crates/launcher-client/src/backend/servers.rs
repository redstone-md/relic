//! The game's own server list (`servers.dat`), so the server a player came
//! for is in the multiplayer menu without typing its address.

use std::path::Path;

const FILE: &str = "servers.dat";
const COMPOUND: u8 = 10;
const LIST: u8 = 9;
const STRING: u8 = 8;
/// Deeper than any real server list; stops a broken file from exhausting the stack.
const MAX_DEPTH: u8 = 32;

/// One NBT value. Strings stay as the bytes the game wrote, so entries the
/// player added come back out exactly as they went in.
#[derive(Debug, Clone, PartialEq)]
enum Tag {
    Byte(u8),
    Short([u8; 2]),
    Int([u8; 4]),
    Long([u8; 8]),
    Float([u8; 4]),
    Double([u8; 8]),
    Bytes(Vec<u8>),
    String(Vec<u8>),
    List(u8, Vec<Tag>),
    Compound(Vec<(Vec<u8>, Tag)>),
    Ints(Vec<u8>),
    Longs(Vec<u8>),
}

impl Tag {
    fn id(&self) -> u8 {
        match self {
            Tag::Byte(_) => 1,
            Tag::Short(_) => 2,
            Tag::Int(_) => 3,
            Tag::Long(_) => 4,
            Tag::Float(_) => 5,
            Tag::Double(_) => 6,
            Tag::Bytes(_) => 7,
            Tag::String(_) => STRING,
            Tag::List(..) => LIST,
            Tag::Compound(_) => COMPOUND,
            Tag::Ints(_) => 11,
            Tag::Longs(_) => 12,
        }
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        if self.0.len() < len {
            return None;
        }
        let (head, rest) = self.0.split_at(len);
        self.0 = rest;
        Some(head)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    fn length(&mut self) -> Option<usize> {
        usize::try_from(i32::from_be_bytes(self.array()?)).ok()
    }

    fn string(&mut self) -> Option<Vec<u8>> {
        let len = u16::from_be_bytes(self.array()?);
        Some(self.take(len.into())?.to_vec())
    }

    fn tag(&mut self, id: u8, depth: u8) -> Option<Tag> {
        if depth > MAX_DEPTH {
            return None;
        }
        Some(match id {
            1 => Tag::Byte(self.array::<1>()?[0]),
            2 => Tag::Short(self.array()?),
            3 => Tag::Int(self.array()?),
            4 => Tag::Long(self.array()?),
            5 => Tag::Float(self.array()?),
            6 => Tag::Double(self.array()?),
            7 => {
                let len = self.length()?;
                Tag::Bytes(self.take(len)?.to_vec())
            }
            STRING => Tag::String(self.string()?),
            LIST => {
                let of = self.array::<1>()?[0];
                let len = self.length()?;
                let mut items = Vec::new();
                for _ in 0..len {
                    items.push(self.tag(of, depth + 1)?);
                }
                Tag::List(of, items)
            }
            COMPOUND => {
                let mut fields = Vec::new();
                loop {
                    let id = self.array::<1>()?[0];
                    if id == 0 {
                        break;
                    }
                    let name = self.string()?;
                    fields.push((name, self.tag(id, depth + 1)?));
                }
                Tag::Compound(fields)
            }
            11 => {
                let len = self.length()?.checked_mul(4)?;
                Tag::Ints(self.take(len)?.to_vec())
            }
            12 => {
                let len = self.length()?.checked_mul(8)?;
                Tag::Longs(self.take(len)?.to_vec())
            }
            _ => return None,
        })
    }
}

fn write_string(out: &mut Vec<u8>, bytes: &[u8]) {
    let len = u16::try_from(bytes.len()).unwrap_or(u16::MAX);
    out.extend(len.to_be_bytes());
    out.extend(&bytes[..len.into()]);
}

fn write_length(out: &mut Vec<u8>, len: usize) {
    out.extend(i32::try_from(len).unwrap_or(i32::MAX).to_be_bytes());
}

fn write(out: &mut Vec<u8>, tag: &Tag) {
    match tag {
        Tag::Byte(v) => out.push(*v),
        Tag::Short(v) => out.extend(v),
        Tag::Int(v) | Tag::Float(v) => out.extend(v),
        Tag::Long(v) | Tag::Double(v) => out.extend(v),
        Tag::Bytes(v) => {
            write_length(out, v.len());
            out.extend(v);
        }
        Tag::String(v) => write_string(out, v),
        Tag::List(of, items) => {
            out.push(*of);
            write_length(out, items.len());
            for item in items {
                write(out, item);
            }
        }
        Tag::Compound(fields) => {
            for (name, value) in fields {
                out.push(value.id());
                write_string(out, name);
                write(out, value);
            }
            out.push(0);
        }
        Tag::Ints(v) => {
            write_length(out, v.len() / 4);
            out.extend(v);
        }
        Tag::Longs(v) => {
            write_length(out, v.len() / 8);
            out.extend(v);
        }
    }
}

/// Java's modified UTF-8: NUL takes two bytes and characters outside the
/// basic plane are written as a surrogate pair.
fn java_utf8(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut units = [0u16; 2];
    for c in text.chars() {
        if c == '\0' {
            out.extend([0xC0, 0x80]);
        } else if (c as u32) < 0x1_0000 {
            let mut buffer = [0u8; 4];
            out.extend(c.encode_utf8(&mut buffer).as_bytes());
        } else {
            for unit in c.encode_utf16(&mut units) {
                out.extend([
                    0xE0 | (*unit >> 12) as u8,
                    0x80 | ((*unit >> 6) & 0x3F) as u8,
                    0x80 | (*unit & 0x3F) as u8,
                ]);
            }
        }
    }
    out
}

fn parse(bytes: &[u8]) -> Option<Tag> {
    let mut reader = Reader(bytes);
    if reader.array::<1>()?[0] != COMPOUND {
        return None;
    }
    reader.string()?;
    reader.tag(COMPOUND, 0)
}

/// The list with `address` in it, or `None` when it is there already or the
/// file is not one this code understands; a file like that is left alone.
fn with_server(existing: Option<&[u8]>, name: &str, address: &str) -> Option<Vec<u8>> {
    let mut root = match existing {
        Some(bytes) => match parse(bytes)? {
            Tag::Compound(fields) => fields,
            _ => return None,
        },
        None => Vec::new(),
    };
    if !root.iter().any(|(key, _)| key == b"servers") {
        root.push((b"servers".to_vec(), Tag::List(COMPOUND, Vec::new())));
    }
    let (_, servers) = root.iter_mut().find(|(key, _)| key == b"servers")?;
    let Tag::List(of, entries) = servers else {
        return None;
    };
    if !entries.is_empty() && *of != COMPOUND {
        return None;
    }
    let listed = entries.iter().any(|entry| match entry {
        Tag::Compound(fields) => fields.iter().any(|(key, value)| {
            key == b"ip"
                && matches!(value, Tag::String(ip) if ip.eq_ignore_ascii_case(address.as_bytes()))
        }),
        _ => false,
    });
    if listed {
        return None;
    }
    *of = COMPOUND;
    entries.insert(
        0,
        Tag::Compound(vec![
            (b"name".to_vec(), Tag::String(java_utf8(name))),
            (b"ip".to_vec(), Tag::String(java_utf8(address))),
        ]),
    );

    let mut out = vec![COMPOUND];
    write_string(&mut out, b"");
    write(&mut out, &Tag::Compound(root));
    Some(out)
}

/// Puts the server first in the game's multiplayer list unless its address is
/// there already. Entries the player added stay as they are.
pub fn remember(game_dir: &Path, name: &str, address: &str) -> std::io::Result<()> {
    let path = game_dir.join(FILE);
    let existing = match fs_err::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(err),
    };
    let Some(updated) = with_server(existing.as_deref(), name, address) else {
        return Ok(());
    };
    let staged = game_dir.join(format!("{FILE}.part"));
    fs_err::write(&staged, updated)?;
    fs_err::rename(&staged, &path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(bytes: &[u8]) -> Vec<(String, String)> {
        let Some(Tag::Compound(root)) = parse(bytes) else {
            panic!("not a server list");
        };
        let Some((_, Tag::List(_, servers))) = root.into_iter().find(|(key, _)| key == b"servers")
        else {
            panic!("no servers");
        };
        servers
            .into_iter()
            .map(|entry| {
                let Tag::Compound(fields) = entry else {
                    panic!("not an entry");
                };
                let text = |wanted: &[u8]| {
                    fields
                        .iter()
                        .find_map(|(key, value)| match value {
                            Tag::String(v) if key == wanted => {
                                Some(String::from_utf8_lossy(v).into_owned())
                            }
                            _ => None,
                        })
                        .unwrap_or_default()
                };
                (text(b"name"), text(b"ip"))
            })
            .collect()
    }

    #[test]
    fn a_missing_list_is_created_with_the_server() {
        let made = with_server(None, "Пещера", "mc.example:25570").unwrap();
        assert_eq!(
            entries(&made),
            [("Пещера".to_owned(), "mc.example:25570".to_owned())]
        );
    }

    #[test]
    fn the_server_goes_first_and_other_entries_survive() {
        let first = with_server(None, "Friends", "friends.example").unwrap();
        // What the game adds next to an entry must come back untouched.
        let Some(Tag::Compound(mut root)) = parse(&first) else {
            panic!("not a server list");
        };
        let Tag::List(_, servers) = &mut root[0].1 else {
            panic!("no servers");
        };
        let Tag::Compound(fields) = &mut servers[0] else {
            panic!("not an entry");
        };
        fields.push((b"acceptTextures".to_vec(), Tag::Byte(1)));
        let mut saved = vec![COMPOUND];
        write_string(&mut saved, b"");
        write(&mut saved, &Tag::Compound(root));

        let both = with_server(Some(&saved), "Relic", "mc.example:25570").unwrap();
        assert_eq!(
            entries(&both),
            [
                ("Relic".to_owned(), "mc.example:25570".to_owned()),
                ("Friends".to_owned(), "friends.example".to_owned()),
            ]
        );
        let Some(Tag::Compound(root)) = parse(&both) else {
            panic!("not a server list");
        };
        let Tag::List(_, servers) = &root[0].1 else {
            panic!("no servers");
        };
        let Tag::Compound(kept) = &servers[1] else {
            panic!("not an entry");
        };
        assert!(kept.contains(&(b"acceptTextures".to_vec(), Tag::Byte(1))));
    }

    #[test]
    fn a_listed_address_is_not_added_twice() {
        let once = with_server(None, "Relic", "mc.example:25570").unwrap();
        assert!(with_server(Some(&once), "Renamed", "MC.Example:25570").is_none());
    }

    #[test]
    fn an_empty_list_the_game_wrote_takes_the_server() {
        // The game writes a list without entries as a list of nothing.
        let mut empty = vec![COMPOUND];
        write_string(&mut empty, b"");
        write(
            &mut empty,
            &Tag::Compound(vec![(b"servers".to_vec(), Tag::List(0, Vec::new()))]),
        );
        let made = with_server(Some(&empty), "Relic", "mc.example:25570").unwrap();
        assert_eq!(entries(&made).len(), 1);
    }

    #[test]
    fn a_file_that_is_not_a_server_list_is_left_alone() {
        assert!(with_server(Some(b"not nbt"), "Relic", "mc.example:25570").is_none());
        assert!(with_server(Some(&[COMPOUND, 0, 0, LIST]), "Relic", "mc.example:25570").is_none());
    }

    #[test]
    fn names_outside_the_basic_plane_use_surrogates() {
        assert_eq!(java_utf8("a"), b"a");
        assert_eq!(java_utf8("\u{1F600}"), [0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80]);
    }
}

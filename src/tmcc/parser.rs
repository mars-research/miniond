//! TMCD response parser.

use std::str::FromStr;
use std::collections::HashMap;

use regex::Regex;

use crate::error::{Result, Error};

/// A TMCD response line.
///
/// A key-value response looks like the following:
///
/// > ADDUSER LOGIN=zhaofeng PSWD=* UID=20001 GID=6418 ROOT=1 NAME="zhaofeng" HOMEDIR=/users/zhaofeng GLIST="" SERIAL=1630039457 EMAIL="root@localhost" SHELL=bash
///
/// We may implement a custom Serde format later.
pub struct Response<'a> {
    line: &'a str,
    response_type: Option<&'a str>,
    kv: HashMap<&'a str, &'a str>,
}

impl<'a> Response<'a> {
    /// Parse a line.
    pub fn parse(line: &'a str) -> Result<Self> {
        let mut response_type: Option<&str> = None;
        let mut kv = HashMap::new();
        let mut first = true;

        let mut rest = line;

        let regex = Regex::new(r#"^(?P<key>[A-Z]+)(=("(?P<quoted_value>[^"]*)"|(?P<value>[^ ]+)))?($| (?P<rest>.+)$)"#).unwrap();

        loop {
            let captures = regex.captures(rest).ok_or(Error::TmcdBadLine {
                line: line.to_string(),
                position: line.len() - rest.len(),
            })?;

            let key = captures.name("key").unwrap().as_str();

            if let Some(value) = captures.name("value") {
                kv.insert(key, value.as_str());
            } else if let Some(quoted_value) = captures.name("quoted_value") {
                kv.insert(key, quoted_value.as_str());
            } else {
                if !first {
                    return Err(Error::TmcdBadLine {
                        line: line.to_string(),
                        position: line.len() - rest.len(),
                    });
                }

                response_type.replace(key);
            }

            first = false;

            if let Some(r) = captures.name("rest") {
                rest = r.as_str();
            } else {
                break;
            }
        }

        Ok(Self {
            line,
            response_type,
            kv,
        })
    }

    /// Returns the response type.
    pub fn response_type(&self) -> &Option<&str> {
        &self.response_type
    }

    /// Parse the value of a key.
    pub fn get_parsed<F: FromStr>(&self, key: &str) -> Result<F>
        where <F as FromStr>::Err: std::error::Error + Send + Sync + 'static,
    {
        if let Some(val) = self.kv.get(key) {
            val.parse::<F>().map_err(|e| {
                Error::TmcdBadValue {
                    value: val.to_string(),
                    parse_error: Box::new(e),
                }
            })
        } else {
            Err(Error::TmcdMissingKey {
                key: key.to_string(),
                line: self.line.to_string(),
            })
        }
    }

    /// Returns the value of a key.
    pub fn get(&self, key: &str) -> Result<&&str> {
        self.kv.get(key).ok_or(Error::TmcdMissingKey {
            key: key.to_string(),
            line: self.line.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adduser() {
        let r = Response::parse(r#"ADDUSER LOGIN=zhaofeng PSWD=* UID=20001 GID=12345 ROOT=1 NAME="Zhaofeng Li" HOMEDIR=/users/zhaofeng GLIST="" SERIAL=1630039457 EMAIL="root@localhost" SHELL=bash"#)
            .expect("Failed to parse");

        assert_eq!("ADDUSER", r.response_type().unwrap());
        assert_eq!("zhaofeng", *r.get("LOGIN").unwrap());
        assert_eq!("Zhaofeng Li", *r.get("NAME").unwrap());
        assert_eq!("", *r.get("GLIST").unwrap());
        assert_eq!("bash", *r.get("SHELL").unwrap());
        assert_eq!(20001, r.get_parsed::<u16>("UID").unwrap());
        assert_eq!(12345, r.get_parsed::<u16>("GID").unwrap());
    }

    #[test]
    fn test_nfs() {
        let r = Response::parse(r#"REMOTE=nfs.emulab:/proj/project-PG0 LOCAL=/proj/project-PG0"#)
            .expect("Failed to parse");

        assert!(r.response_type().is_none());
        assert_eq!("nfs.emulab:/proj/project-PG0", *r.get("REMOTE").unwrap());
        assert_eq!("/proj/project-PG0", *r.get("LOCAL").unwrap());
    }
}

use log::debug;
use serde::Deserialize;
use std::fmt::{self, Debug, Formatter};
use std::fs::File;
use std::io::{Read, Result};

#[derive(Deserialize, Clone)]
pub enum PasswordContainer {
    Plaintext(String),
    Shell(String),
}

#[derive(Deserialize, Debug, Clone)]
pub enum Method {
    StartTls,
    Tls,
}

fn zero() -> usize {
    0
}

#[derive(Deserialize, Clone)]
pub struct Account {
    pub host: String,
    pub port: u16,
    pub method: Method,
    pub username: String,
    pub password: PasswordContainer,
    pub folder: String,
    #[serde(default = "zero")]
    pub connections: usize,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub accounts: Vec<Account>,
}

impl Config {
    pub fn read_from(path: &str) -> Result<Config> {
        let mut f = File::open(path)?;
        debug!("config file open");
        let mut buf = String::new();
        f.read_to_string(&mut buf)?;
        debug!("config file text read");
        Ok(toml::from_str(&buf)?)
    }
}

impl Debug for PasswordContainer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        use PasswordContainer::*;
        match &self {
            Plaintext(_) => write!(f, "[plaintext password]"),
            Shell(_) => write!(f, "[shell command]"),
        }
    }
}

// we want the Debug impl to not print user details
impl Debug for Account {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Account {{ method: {:?}, port: {:?}, connections: {:?} }}",
            self.method, self.port, self.connections,
        )
    }
}

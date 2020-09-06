use super::config::*;
use imap::types::Flag;
use imap::types::Seq;
use maildir::Maildir;
use native_tls::{TlsConnector, TlsStream};
use std::collections::HashSet;
use std::ffi::OsString;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;

quick_error! {
    #[derive(Debug)]
    pub enum MailError {
        Io(err: std::io::Error) {
            from()
        }
        ImapError(err: imap::error::Error) {
            from()
        }
        TlsError(err: native_tls::Error) {
            from()
        }
    }
}

fn connect(acc: &Account) -> Result<imap::Client<TlsStream<TcpStream>>, MailError> {
    use Method::*;

    let host: &str = &acc.host;
    let port = acc.port;
    let tls = TlsConnector::new()?;
    match &acc.method {
        Tls => imap::connect((host, port), host, &tls).map_err(MailError::from),
        StartTls => imap::connect_starttls((host, port), host, &tls).map_err(MailError::from),
    }
}

fn retrieve_password(pc: &PasswordContainer) -> std::io::Result<String> {
    use PasswordContainer::*;
    match pc {
        Plaintext(p) => Ok(p.clone()),
        Shell(cmd) => {
            debug!("start shell command to retrieve password {}", &cmd);
            let mut spl = shlex::split(&cmd)
                .expect("failed executing password command")
                .into_iter()
                .map(OsString::from);
            Command::new(spl.next().unwrap())
                .args(spl)
                .output()
                // TODO check if command was successful via o.status.is_success
                .map(|o| {
                    String::from_utf8(o.stdout)
                        .expect("password command returned non-utf text on stdout")
                })
                .map(|s| s.trim().to_owned())
        }
    }
}

fn to_imap_seq(seq: HashSet<Seq>) -> String {
    format!(
        "({})",
        seq.into_iter().fold("".to_string(), |acc, x| {
            if acc.is_empty() {
                format!("{}", x)
            } else {
                format!("{} {}", acc, x)
            }
        })
    )
}

fn init_maildir<S1, S2>(folder: S1, mailbox: S2) -> std::io::Result<Maildir>
where
    S1: AsRef<str>,
    S2: AsRef<str>,
{
    let mut p = PathBuf::new();
    p.push(folder.as_ref());
    p.push(mailbox.as_ref());
    debug!("ensure maildir exists at {:?}", p);

    let md = Maildir::from(p);
    md.create_dirs()?;
    Ok(md)
}

struct MaildirFlag(char);

impl MaildirFlag {
    fn as_char(&self) -> char {
        self.0
    }

    fn from<'a>(f: &Flag<'a>) -> Option<MaildirFlag> {
        use Flag::*;
        match f {
            Seen => Some(MaildirFlag('S')),
            Answered => Some(MaildirFlag('R')),
            Flagged => Some(MaildirFlag('F')),
            Deleted => Some(MaildirFlag('T')),
            Draft => Some(MaildirFlag('D')),
            _ => None,
        }
    }
}

pub fn flags_for_maildir(flags: &[Flag<'_>]) -> String {
    flags
        .into_iter()
        .filter_map(MaildirFlag::from)
        .map(|mf| mf.as_char())
        .collect()
}

pub async fn get(acc: Account) {
    debug!("connecting to mailserver for {:?}", &acc);
    let pass = retrieve_password(&acc.password).expect("unable to retrieve password");
    let client = match connect(&acc) {
        Ok(c) => c,
        Err(_) => panic!("failed to connect to mail server {}:{}", acc.host, acc.port),
    };
    debug!("connected!");

    debug!("logging in...");
    let mut session = client
        .login(acc.username, pass)
        .expect("failed to login: incorrect credentials");
    debug!("login ok!");
    debug!("listing all");

    let names = session.list(None, Some("*")).unwrap();

    for name in &names {
        let maildir = init_maildir(&acc.folder, name.name()).unwrap();

        debug!("examine mailbox {}", name.name());
        let mailbox = session.examine(name.name()).unwrap();
        debug!("examine: {:?}", mailbox);

        let search = session.search("ALL").unwrap();
        debug!("search: {:?}", search);

        for uid in search {
            let fetch = session.fetch(format!("{}", uid), "BODY[]").unwrap();
            assert_eq!(fetch.len(), 1);
            let flags = flags_for_maildir(fetch[0].flags());
            maildir
                .store_cur_with_flags(fetch[0].body().unwrap(), &flags)
                .unwrap();
        }
    }
}

use super::config::*;
use anyhow::{anyhow, Context, Result};
use imap::types::Flag;
use imap::Session;
use log::{debug, error, info};
use maildir::Maildir;
use native_tls::{TlsConnector, TlsStream};
use quick_error::quick_error;
use std::ffi::OsString;
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::Command;
use tokio::sync::mpsc;

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

fn connect(acc: &Account) -> Result<imap::Client<TlsStream<TcpStream>>> {
    use Method::*;

    let host: &str = &acc.host;
    let port = acc.port;
    let tls = TlsConnector::new()?;
    match &acc.method {
        Tls => Ok(imap::connect((host, port), host, &tls).map_err(MailError::from)?),
        StartTls => Ok(imap::connect_starttls((host, port), host, &tls).map_err(MailError::from)?),
    }
}

fn retrieve_password(pc: &PasswordContainer) -> Result<String> {
    use PasswordContainer::*;
    match pc {
        Plaintext(p) => Ok(p.clone()),
        Shell(cmd) => Ok({
            debug!("start shell command to retrieve password {}", &cmd);
            let mut spl = shlex::split(&cmd)
                .context("failed executing password command")?
                .into_iter()
                .map(OsString::from);
            Command::new(
                spl.next()
                    .ok_or(anyhow!("The shell command for password is required"))?,
            )
            .args(spl)
            .output()
            .map(|o| {
                String::from_utf8(o.stdout)
                    .expect("password command returned non-utf text on stdout")
            })
            .map(|s| s.trim().to_owned())
        }?),
    }
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

fn flags_for_maildir(flags: &[Flag<'_>]) -> String {
    flags
        .into_iter()
        .filter_map(MaildirFlag::from)
        .map(|mf| mf.as_char())
        .collect()
}

fn establish_session(acc: &Account, pass: &str) -> Session<TlsStream<TcpStream>> {
    debug!("connecting to mailserver for {:?}", &acc);
    let client = match connect(&acc) {
        Ok(c) => c,
        Err(_) => panic!("failed to connect to mail server {}:{}", acc.host, acc.port),
    };
    debug!("connected!");

    debug!("logging in...");
    let session = client
        .login(&acc.username, pass)
        .expect("failed to login: incorrect credentials");
    debug!("login ok!");
    session
}

async fn get_mailbox(acc: Account, name: String, pass: String) -> Result<()> {
    info!("download mailbox {}", name);
    let maildir = init_maildir(&acc.folder, &name).unwrap();
    let mut session = establish_session(&acc, &pass);

    let mailbox = session.examine(&name)?;
    debug!("examine: {:?}", mailbox);

    // TODO: check local state for mailbox and alter search term based on that
    let search = session.search("ALL")?;
    debug!("search: {:?}", search);
    info!("({}) emails found", search.len());

    let mut handles = Vec::new();
    {
        let (tx, mut rx) = mpsc::channel(acc.connections.into());

        let conn: usize = acc.connections.into();
        let workset_size: usize = search.len() / (conn);
        if workset_size == 0 {
            info!("mailbox {} is empty, nothing to fetch", &name);
            return Ok(());
        }

        let whole_workset: Vec<_> = search.into_iter().collect();
        let chunks = whole_workset.as_slice().chunks(workset_size);

        for chunk in chunks {
            let acc = acc.clone();
            let pass = pass.clone();
            let name = name.clone();
            let workset: Vec<_> = chunk.to_vec();
            let tx = tx.clone();
            debug!("spawn thread for mailbox {}", name);

            handles.push(tokio::spawn(async move {
                let mut session = establish_session(&acc, &pass);
                session
                    .examine(&name)
                    .with_context(|| format!("Failed to examine session {}", name))?;

                for uid in workset {
                    let fetch = session
                        .fetch(format!("{}", uid), "BODY[]")
                        .with_context(|| format!("fetching mail {} failed", uid))?;
                    assert_eq!(fetch.len(), 1);
                    debug!("fetched mail {}, awaiting save", uid);
                    tx.send(fetch)
                        .await
                        .with_context(|| format!("saving mail {} failed", uid))?;
                    debug!("mail {} saved, continue", uid);
                }
                debug!("thread is done");

                Ok(())
            }));
        }

        handles.push(tokio::spawn(async move {
            while let Some(mail) = rx.recv().await {
                let mail = &mail[0];
                let flags = flags_for_maildir(mail.flags());
                maildir
                    .store_cur_with_flags(
                        mail.body().with_context(|| {
                            format!("failed to retrieve body for mail {:?}", mail.uid)
                        })?,
                        &flags,
                    )
                    .with_context(|| format!("failed to store mail {:?}", mail.uid))?;
                debug!("mail saved, awaiting fetch");
            }

            Result::<(), anyhow::Error>::Ok(())
        }));
    }

    futures::future::try_join_all(handles).await?;
    Ok(())
}

pub async fn get(acc: Account) {
    let pass = retrieve_password(&acc.password).expect("unable to retrieve password");
    let mut session = establish_session(&acc, &pass);
    debug!("listing all");

    let names = session.list(None, Some("*")).unwrap();
    let mut handles = Vec::new();
    for name in &names {
        let n = name.name().to_string();
        let acc = acc.clone();
        let pass = pass.clone();
        handles.push(get_mailbox(acc, n, pass));
    }

    for handle in handles {
        match handle.await {
            Err(e) => error!("an unexpected error has occured: {:?}", e),
            _ => {}
        }
    }
}

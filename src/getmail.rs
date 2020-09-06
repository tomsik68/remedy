use super::config::*;
use native_tls::{TlsConnector, TlsStream};
use std::ffi::OsString;
use std::net::TcpStream;
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
                .map(|o| {
                    String::from_utf8(o.stdout)
                        .expect("password command returned non-utf text on stdout")
                })
                .map(|s| s.trim().to_owned())
        }
    }
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
    let names = session.list(Some("*"), Some("*")).unwrap();
    for name in &names {
        debug!("result: {}", name.name());
    }
}

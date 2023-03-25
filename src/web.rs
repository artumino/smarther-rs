use actix_web::{get, web::Data};
use crossbeam::channel::Sender;

#[derive(Debug, Clone)]
pub struct AuthState {
    pub auth_channel: Sender<anyhow::Result<String>>,
    pub cross_code: String
}

#[get("/tokens")]
async fn tokens(code: Option<String>, state: Option<String>, data: Data<AuthState>) -> &'static str {
    let result = match code {
        Some(code) => Ok(code),
        None => Err(anyhow::anyhow!("Error during authorization"))
    };

    let is_ok = result.is_ok() && state.map(|state| data.cross_code == state).unwrap_or(false);

    if is_ok {
        data.auth_channel.send(result).unwrap();
        "Authorized! You can close this window now."
    } else {
        data.auth_channel.send(Err(anyhow::anyhow!("Error during authorization"))).unwrap();
        "Error during authorization"
    }
}
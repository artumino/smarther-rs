use actix_web::{get, web::{Data, Query}};
use crossbeam::channel::Sender;

#[derive(Debug, Clone)]
pub struct AuthState {
    pub auth_channel: Sender<anyhow::Result<String>>,
    pub csrf_token: String
}

#[derive(Deserialize)]
struct AuthenticationResponse {
    code: Option<String>,
    state: Option<String>
}

#[get("/tokens")]
async fn tokens(query: Query<AuthenticationResponse>, data: Data<AuthState>) -> &'static str {
    let query = query.into_inner();
    extract_tokens(&query, &data.csrf_token, &data.auth_channel).await
}

async fn extract_tokens(auth_info: &AuthenticationResponse, csrf_token: &String, auth_channel: &Sender<anyhow::Result<String>>) -> &'static str {
    let result = match &auth_info {
        AuthenticationResponse {
            code: Some(code), 
            state: Some(state) 
        } => {
            if state == csrf_token {
                Ok(code.clone())
            } else {
                Err(anyhow::anyhow!("Error during authorization"))
            } 
        },
        _ => Err(anyhow::anyhow!("Error during authorization"))
    };
    
    if result.is_ok() {
        auth_channel.send(result).unwrap();
        "Authorized! You can close this window now."
    } else {
        auth_channel.send(Err(anyhow::anyhow!("Error during authorization"))).unwrap();
        "Error during authorization"
    }
}

#[cfg(test)]
mod test {
    use crate::web::AuthenticationResponse;

    #[tokio::test]
    async fn token_works() -> anyhow::Result<()> {
        use crossbeam::channel::bounded;

        let (tx, rx) = bounded::<anyhow::Result<String>>(1);
        let token_code: String = "test_token".into();
        let csrf_token = "48da44ff-98fe-40c8-9a55-ac186decbf6f".to_string();
        let auth_info = AuthenticationResponse {
            code: Some(token_code.clone()),
            state: Some(csrf_token.clone())
        };
        let result = super::extract_tokens(&auth_info,
            &csrf_token,
            &tx)
            .await;

        let received = rx.recv().unwrap();
        assert_eq!(result, "Authorized! You can close this window now.");
        assert!(received.is_ok());

        Ok(())
    }
}
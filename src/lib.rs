#[macro_use] extern crate serde;

use std::{collections::HashMap, time::SystemTime};

use anyhow::anyhow;
use jwt::{Claims, Token, Header};
use reqwest::Client;
use states::{Unauthorized, Authorized};

pub const API_URL: &str = "https://api.developer.legrand.com/smarther/v2.0/";
pub const AUTH_URL: &str = "https://partners-login.eliotbylegrand.com/authorize";
pub const TOKEN_URL: &str = "https://partners-login.eliotbylegrand.com/token";

mod states {
    pub struct Unauthorized;
    pub struct Authorized;
}

#[cfg(feature = "web")]
mod web;

#[derive(Debug, Deserialize, Serialize, PartialEq, Hash, PartialOrd, Clone)]
pub enum AuthorizationGrant {
    None,
    AccessCode {
        client_id: String,
        client_secret: String,
        access_code: String
    },
    OAuthToken(String)
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Hash, PartialOrd, Clone)]
pub struct AuthorizationInfo {
    pub grant: AuthorizationGrant,
    pub subscription_key: String
}

impl AuthorizationInfo {
    pub fn new(subscription_key: &str) -> Self {
        Self {
            grant: AuthorizationGrant::None,
            subscription_key: subscription_key.to_string()
        }
    }

    pub fn get_request_token(&self) -> anyhow::Result<String> {
        if let AuthorizationGrant::OAuthToken(ref jwt) = self.grant {
            let token: Token<Header, Claims, _> = jwt::Token::parse_unverified(jwt)?;
            let claims = token.claims();
            let expiration_date = claims.registered.expiration.ok_or(anyhow!("Missing token expiration date"))?;
            let access_token = claims.private.get("access_token").ok_or(anyhow!("Missing access token"))?.to_string();
            if expiration_date > SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs() {
                return Ok(access_token);
            }
        }
        Err(anyhow!("No valid request token found"))
    }

    pub fn get_refresh_token(&self) -> anyhow::Result<String> {
        if let AuthorizationGrant::OAuthToken(ref jwt) = self.grant {
            let token: Token<Header, Claims, _> = jwt::Token::parse_unverified(jwt)?;
            let claims = token.claims();
            return Ok(claims.private.get("refresh_token").ok_or(anyhow!("Missing access token"))?.to_string());
        }
        Err(anyhow!("No valid refresh token found"))
    }
}

pub struct SmartherApi<State> {
    auth_info: Option<AuthorizationInfo>,
    client: Client,
    state: std::marker::PhantomData<State>,
}

impl Default for SmartherApi<Unauthorized> {
    fn default() -> Self {
        Self {
            auth_info: None,
            client: Client::new(),
            state: std::marker::PhantomData,
        }
    }
}

impl SmartherApi<Unauthorized> {
    #[cfg(feature = "web")]
    pub async fn authorize_oauth(self, client_id: &str, client_secret: &str, base_uri: Option<&str>, subscription_key: &str) -> anyhow::Result<SmartherApi<Authorized>> {
        use actix_web::{App, HttpServer, web::Data};

        let (tx, rx) = crossbeam::channel::bounded::<anyhow::Result<String>>(1);

        let cross_code = uuid::Uuid::new_v4().to_string();
        let auth_state = web::AuthState {
            auth_channel: tx,
            cross_code: cross_code.clone()
        };

        let redirect_url = format!("{}/tokens", base_uri.unwrap_or("http://localhost:23784"));
        let auth_code = tokio::select!(
            code = async move {
                open::that(format!("{AUTH_URL}?response_type=code&client_id={client_id}&state={cross_code}&redirect_uri={redirect_url}")).unwrap();

                let code = tokio::task::spawn_blocking(move || {
                    rx.recv().unwrap()
                }).await;
                code?

            } => code,
            _ = async move {
                HttpServer::new(move || {
                    App::new()
                        .app_data(Data::new(auth_state.clone()))
                        .service(web::tokens)
                })
                .bind(("localhost", 23784))?
                .run()
                .await
            } => Err(anyhow::anyhow!("Error binding local server to port 23784"))
        )?;

        self.authorize(AuthorizationInfo { 
            grant: AuthorizationGrant::AccessCode { 
                client_id: client_id.to_string(), 
                client_secret: client_secret.to_string(), 
                access_code: auth_code
            }, 
            subscription_key: subscription_key.to_string()
        }).await
    }

    async fn refresh_if_needed(&self, auth_info: &AuthorizationInfo) -> anyhow::Result<AuthorizationInfo> {
        if auth_info.get_request_token().is_ok() {
            return Ok(auth_info.clone());
        }
        
        let refresh_token = auth_info.get_refresh_token();
        let response = match auth_info.grant {
            AuthorizationGrant::OAuthToken(_) => {
                self.client.get(TOKEN_URL)
                    .query(&[
                        ("grant_type", "refresh_token"),
                        ("refresh_token", &refresh_token?)
                    ])
                    .send().await?
            },
            AuthorizationGrant::AccessCode { ref client_id, ref client_secret, ref  access_code } => {
                self.client.get(TOKEN_URL)
                    .query(&[
                        ("grant_type", "authorization_code"),
                        ("client_id", client_id),
                        ("client_secret", client_secret),
                        ("code", access_code)
                    ])
                    .send().await?
            },
            _ => { return Ok(auth_info.clone()) }
        };

        match response.status() {
            reqwest::StatusCode::OK => (),
            _ => { return Err(anyhow::anyhow!(response.status().to_string())) }
        }

        let token = response.text().await?;
        Ok(AuthorizationInfo {
            grant: AuthorizationGrant::OAuthToken(token),
            subscription_key: "test".to_string()
        })
    }

    pub async fn authorize(self, auth_info: AuthorizationInfo) -> anyhow::Result<SmartherApi<Authorized>> {
        let refreshed_token = self.refresh_if_needed(&auth_info).await?;

        if refreshed_token.grant == AuthorizationGrant::None {
            return Err(anyhow::anyhow!("Invalid authorization grant"));
        }

        Ok(SmartherApi {
            auth_info: Some(refreshed_token),
            client: self.client,
            state: std::marker::PhantomData,
        })
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Hash, PartialOrd, Clone)]
pub struct Plants
{
    pub plants: Vec<Plant>
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Hash, PartialOrd, Clone)]
pub struct Plant
{
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub plant_type: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct PlantDetail
{
    pub id: String,
    pub name: String,
    pub modules: Vec<Module>
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Module
{
    pub device: String,
    pub name: String,
    pub id: String,
    pub capabilities: Vec<ModuleCapability>
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct ModuleCapability
{
    capability: String,

    #[serde(flatten)]
    pub can_do: HashMap<String, serde_json::Value>
}

impl SmartherApi<Authorized> {
    pub fn auth_info(&self) -> Option<&AuthorizationInfo> {
        self.auth_info.as_ref()
    }

    fn auth_header(&self) -> anyhow::Result<(&'static str, String)> {
        let auth_info = self.auth_info.as_ref().ok_or(anyhow!("Client should be authorized"))?;
        Ok(("Authorization" , format!("Bearer {}", auth_info.get_request_token()?)))
    }

    fn subscription_header(&self) -> anyhow::Result<(&'static str, String)> {
        let auth_info = self.auth_info.as_ref().ok_or(anyhow!("Client should be authorized"))?;
        Ok(("Ocp-Apim-Subscription-Key", auth_info.subscription_key.clone()))
    }

    fn smarther_headers(&self) -> anyhow::Result<reqwest::header::HeaderMap> {
        let mut headers = reqwest::header::HeaderMap::new();
        let auth_header = self.auth_header()?;
        let subscription_header = self.subscription_header()?;
        headers.insert(auth_header.0, auth_header.1.parse()?);
        headers.insert(subscription_header.0, subscription_header.1.parse()?);
        Ok(headers)
    }

    pub async fn get_plants(&self) -> anyhow::Result<Plants> {
        let response = self.client.get(format!("{}/plants", API_URL))
            .headers(self.smarther_headers()?)
            .send().await?;

        match response.status() {
            reqwest::StatusCode::OK => (),
            _ => { return Err(anyhow::anyhow!(response.status().to_string())) }
        }
        
        Ok(response.json().await?)
    }

    pub async fn get_topology(&self, plant_id: &str) -> anyhow::Result<PlantDetail> {
        let response = self.client.get(format!("{}/plants/{}/topology", API_URL, plant_id))
            .headers(self.smarther_headers()?)
            .send().await?;

        match response.status() {
            reqwest::StatusCode::OK => (),
            _ => { return Err(anyhow::anyhow!(response.status().to_string())) }
        }
        
        Ok(response.json().await?)
    }
}
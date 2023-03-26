#[macro_use] extern crate serde;

use std::{collections::HashMap, time::SystemTime};

use anyhow::anyhow;
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
#[serde(untagged)]
pub enum AuthorizationGrant {
    None,
    AccessCode {
        access_code: String
    },
    OAuthToken {
        access_token: String,
        refresh_token: String,
        expires_on: u64
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Hash, PartialOrd, Clone)]
pub struct AuthorizationInfo {
    pub grant: AuthorizationGrant,
    pub client_id: String,
    pub client_secret: String,
    pub subscription_key: String
}

impl AuthorizationGrant {
    pub fn request_token(&self) -> anyhow::Result<String> {
        if let AuthorizationGrant::OAuthToken { access_token, expires_on, .. } = self {
            if *expires_on > SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_secs() {
                return Ok(access_token.clone());
            }
        }
        Err(anyhow!("No valid request token found"))
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

#[derive(Serialize, Debug, Clone, Default)]
struct OAuthTokenRequest {
    pub grant_type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

impl TryFrom<&AuthorizationInfo> for OAuthTokenRequest {
    type Error = anyhow::Error;

    fn try_from(info: &AuthorizationInfo) -> Result<Self, Self::Error> {
        let grant = &info.grant;
        let client_id = &info.client_id;
        let client_secret = &info.client_secret;
        match grant {
            AuthorizationGrant::OAuthToken { ref refresh_token, .. } => {
                Ok(OAuthTokenRequest {
                    grant_type: "refresh_token",
                    client_id: Some(client_id.clone()),
                    client_secret: Some(client_secret.clone()),
                    refresh_token: Some(refresh_token.clone()),
                    ..Default::default()
                })
            },
            AuthorizationGrant::AccessCode { ref  access_code } => {
                Ok(OAuthTokenRequest {
                    grant_type: "authorization_code",
                    client_id: Some(client_id.clone()),
                    client_secret: Some(client_secret.clone()),
                    code: Some(access_code.clone()),
                    ..Default::default()
                })
            },
            _ => { Err(anyhow!("Unsupported grant type")) }
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
            csrf_token: cross_code.clone()
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
            client_id: client_id.to_string(), 
            client_secret: client_secret.to_string(), 
            grant: AuthorizationGrant::AccessCode { 
                access_code: auth_code
            }, 
            subscription_key: subscription_key.to_string()
        }).await
    }

    async fn refresh_if_needed(&self, auth_info: &AuthorizationInfo) -> anyhow::Result<AuthorizationInfo> {
        if auth_info.grant.request_token().is_ok() {
            return Ok(auth_info.clone());
        }
        
        let refresh_request: OAuthTokenRequest = auth_info.try_into()?;
        let response = self.client.post(TOKEN_URL)
            .form(&refresh_request)
            .send().await?;

        match response.status() {
            reqwest::StatusCode::OK => (),
            _ => { return Err(anyhow::anyhow!(response.status().to_string())) }
        }

        let token = response.text().await?;
        let auth_token = serde_json::from_str(&token)?;
        Ok(AuthorizationInfo {
            grant: auth_token,
            ..auth_info.clone()
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
        Ok(("Authorization" , format!("Bearer {}", auth_info.grant.request_token()?)))
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

#[cfg(test)]
mod test {
    use crate::{AuthorizationGrant, OAuthTokenRequest, AuthorizationInfo};

    #[test]
    fn request_access_code() {
        let fake_info = &AuthorizationInfo {
            grant: AuthorizationGrant::AccessCode { 
                access_code: "secret_code".into() 
            },
            client_id: "test".into(), 
            client_secret: "secret".into(),
            subscription_key: "sub".into()
        };

        let refresh_request: OAuthTokenRequest = fake_info.try_into().unwrap();
        assert_eq!(refresh_request.grant_type, "authorization_code");
        assert_eq!(refresh_request.client_id, Some("test".into()));
        assert_eq!(refresh_request.client_secret, Some("secret".into()));
        assert_eq!(refresh_request.code, Some("secret_code".into()));
        assert_eq!(refresh_request.refresh_token, None);

        assert_eq!(serde_json::to_string_pretty(&refresh_request).unwrap(), "{\n  \"grant_type\": \"authorization_code\",\n  \"client_id\": \"test\",\n  \"client_secret\": \"secret\",\n  \"code\": \"secret_code\"\n}");
    }

    #[test]
    fn request_refresh_token() {
        let fake_info = &AuthorizationInfo {
            grant: AuthorizationGrant::OAuthToken { 
                access_token: "none".into(), 
                refresh_token: "refresh".into(), 
                expires_on: 0 
            },
            client_id: "test".into(), 
            client_secret: "secret".into(),
            subscription_key: "sub".into()
        };

        let refresh_request: OAuthTokenRequest = fake_info.try_into().unwrap();
        assert_eq!(refresh_request.grant_type, "refresh_token");
        assert_eq!(refresh_request.client_id, Some("test".into()));
        assert_eq!(refresh_request.client_secret, Some("secret".into()));
        assert_eq!(refresh_request.code, None);
        assert_eq!(refresh_request.refresh_token, Some("refresh".into()));

        assert_eq!(serde_json::to_string_pretty(&refresh_request).unwrap(), "{\n  \"grant_type\": \"refresh_token\",\n  \"client_id\": \"test\",\n  \"client_secret\": \"secret\",\n  \"refresh_token\": \"refresh\"\n}");
    }
}
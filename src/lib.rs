#[macro_use] extern crate serde;

use std::time::SystemTime;

use anyhow::anyhow;
use reqwest::Client;
use states::*;
use model::*;

pub const API_URL: &str = "https://api.developer.legrand.com/smarther/v2.0";
pub const AUTH_URL: &str = "https://partners-login.eliotbylegrand.com/authorize";
pub const TOKEN_URL: &str = "https://partners-login.eliotbylegrand.com/token";

#[cfg(test)]
mod test;
mod model;
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

    pub fn needs_refresh(&self) -> bool {
        if let AuthorizationGrant::OAuthToken { expires_on, .. } = self {
            *expires_on < SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
        } else {
            true
        }
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
    pub async fn get_oauth_access_code(&self, client_id: &str, client_secret: &str, base_uri: Option<&str>, subscription_key: &str) -> anyhow::Result<AuthorizationInfo> {
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

        Ok(AuthorizationInfo { 
            client_id: client_id.to_string(), 
            client_secret: client_secret.to_string(), 
            grant: AuthorizationGrant::AccessCode { 
                access_code: auth_code
            }, 
            subscription_key: subscription_key.to_string()
        })
    }

    pub async fn refresh_token(&self, auth_info: &AuthorizationInfo) -> anyhow::Result<AuthorizationInfo> {
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
        if auth_info.grant.needs_refresh() {
            return Err(anyhow!("Authorization needs to be refreshed"))
        }

        Ok(SmartherApi {
            auth_info: Some(auth_info),
            client: self.client,
            state: std::marker::PhantomData,
        })
    }
}

impl SmartherApi<Authorized> {
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
        let response = self.client.get(format!("{API_URL}/plants"))
            .headers(self.smarther_headers()?)
            .send().await?;

        let status = response.status();
        match status {
            reqwest::StatusCode::OK => (),
            _ => { return Err(anyhow::anyhow!(status.to_string())) }
        }
        
        Ok(response.json().await?)
    }

    pub async fn get_topology(&self, plant_id: &str) -> anyhow::Result<PlantTopology> {
        let response = self.client.get(format!("{API_URL}/plants/{plant_id}/topology"))
            .headers(self.smarther_headers()?)
            .send().await?;

        let status = response.status();
        match status {
            reqwest::StatusCode::OK => (),
            _ => { return Err(anyhow::anyhow!(status.to_string())) }
        }
        
        Ok(response.json().await?)
    }

    pub async fn get_device_status(&self, plant_id: &str, module_id: &str) -> anyhow::Result<ModuleStatus> {
        let response = self.client.get(format!("{API_URL}/chronothermostat/thermoregulation/addressLocation/plants/{plant_id}/modules/parameter/id/value/{module_id}"))
            .headers(self.smarther_headers()?)
            .send().await?;

        let status = response.status();
        match status {
            reqwest::StatusCode::OK => (),
            _ => { return Err(anyhow::anyhow!(status.to_string())) }
        }
        
        Ok(response.json().await?)
    }
}
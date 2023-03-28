use crate::{
    model::*,
    AuthorizationGrant, AuthorizationInfo, OAuthTokenRequest,
};

#[test]
fn request_access_code() {
    let fake_info = &AuthorizationInfo {
        grant: AuthorizationGrant::AccessCode {
            access_code: "secret_code".into(),
        },
        client_id: "test".into(),
        client_secret: "secret".into(),
        subscription_key: "sub".into(),
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
            expires_on: 0,
        },
        client_id: "test".into(),
        client_secret: "secret".into(),
        subscription_key: "sub".into(),
    };

    let refresh_request: OAuthTokenRequest = fake_info.try_into().unwrap();
    assert_eq!(refresh_request.grant_type, "refresh_token");
    assert_eq!(refresh_request.client_id, Some("test".into()));
    assert_eq!(refresh_request.client_secret, Some("secret".into()));
    assert_eq!(refresh_request.code, None);
    assert_eq!(refresh_request.refresh_token, Some("refresh".into()));

    assert_eq!(serde_json::to_string_pretty(&refresh_request).unwrap(), "{\n  \"grant_type\": \"refresh_token\",\n  \"client_id\": \"test\",\n  \"client_secret\": \"secret\",\n  \"refresh_token\": \"refresh\"\n}");
}

#[test]
fn measurements_are_parsed_correctly() {
    let celsius = r#"{"unit":"C","value":25.0}"#;
    let fahrenheit = r#"{"unit":"F","value":"77.0"}"#;
    let percentage = r#"{"unit":"%","value":50.0}"#;

    let celsius: Measurement = serde_json::from_str(celsius).unwrap();
    let fahrenheit: Measurement = serde_json::from_str(fahrenheit).unwrap();
    let percentage: Measurement = serde_json::from_str(percentage).unwrap();

    assert_eq!(celsius, Measurement::Celsius(25.0));
    assert_eq!(fahrenheit, Measurement::Fahrenheit(77.0));
    assert_eq!(percentage, Measurement::Percentage(50.0));
}

#[test]
fn timed_measurements_are_parsed_correctly() {
    let celsius = r#"{"unit":"C","value":25.0,"timeStamp":"2020-12-01T00:00:00Z"}"#;
    let fahrenheit = r#"{"unit":"F","value":"77.00000","timeStamp":"2020-12-01T00:00:00Z"}"#;
    let percentage = r#"{"unit":"%","value":50.0,"timeStamp":"2020-12-01T00:00:00Z"}"#;

    let celsius: TimedMeasurement = serde_json::from_str(celsius).unwrap();
    let fahrenheit: TimedMeasurement = serde_json::from_str(fahrenheit).unwrap();
    let percentage: TimedMeasurement = serde_json::from_str(percentage).unwrap();

    assert_eq!(
        celsius,
        TimedMeasurement {
            time_stamp: "2020-12-01T00:00:00Z".parse().unwrap(),
            value: Measurement::Celsius(25.0)
        }
    );

    assert_eq!(
        fahrenheit,
        TimedMeasurement {
            time_stamp: "2020-12-01T00:00:00Z".parse().unwrap(),
            value: Measurement::Fahrenheit(77.0)
        }
    );

    assert_eq!(
        percentage,
        TimedMeasurement {
            time_stamp: "2020-12-01T00:00:00Z".parse().unwrap(),
            value: Measurement::Percentage(50.0)
        }
    );
}

#[test]
fn correctly_parse_status() {
    let status_message_json = std::fs::read_to_string("validation/status_message.json").unwrap();
    let status: ModuleStatus = serde_json::from_str(&status_message_json).unwrap();
    assert!(status.chronothermostats.len() == 1);
}

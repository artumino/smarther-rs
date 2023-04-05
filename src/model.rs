use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_aux::prelude::*;

#[derive(Debug, Deserialize, Serialize, PartialEq, Hash, PartialOrd, Clone)]
pub struct Plants {
    pub plants: Vec<Plant>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Hash, PartialOrd, Clone)]
pub struct Plant {
    pub id: String,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub plant_type: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct PlantTopology {
    pub plant: PlantDetail,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct PlantDetail {
    pub id: String,
    pub name: String,
    pub modules: Vec<Module>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Module {
    pub device: String,
    pub name: String,
    pub id: String,
    pub capabilities: Option<Vec<ModuleCapability>>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct ModuleCapability {
    capability: Option<String>,

    #[serde(flatten)]
    pub can_do: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct ModuleStatus {
    pub chronothermostats: Vec<ThermostatStatus>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ThermostatStatus {
    pub function: ThermostatFunction,
    pub mode: ThermostatMode,
    pub set_point: Option<Measurement>,
    pub programs: Option<Vec<ProgramIdentifier>>,
    pub activation_time: Option<DateTime<Utc>>,
    pub temperature_format: Option<MeasurementUnit>,
    pub load_state: Option<LoadState>,
    pub time: DateTime<Utc>,
    pub thermometer: Option<Instrument>,
    pub hygrometer: Option<Instrument>,
    pub sender: Option<SenderInfo>,
    pub receiver: Option<ReceiverInfo>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct ProgramIdentifier {
    #[serde(deserialize_with = "deserialize_number_from_string")]
    pub number: u32,
}
#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SenderInfo {
    pub address_type: Option<String>,
    pub system: Option<String>,
    pub plant: Option<PlantMinimalDetails>,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReceiverInfo {
    pub oid: Option<Vec<String>>
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "UPPERCASE")]
pub enum ThermostatFunction {
    Heating,
    Cooling,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "UPPERCASE")]
pub enum ThermostatMode {
    Automatic,
    Manual,
    Boost,
    Off,
    Protection,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "UPPERCASE")]
pub enum LoadState {
    Active,
    Inactive,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct Instrument {
    pub measures: Option<Vec<TimedMeasurement>>,
}

impl Instrument {
    pub fn last_measurement(&self) -> Option<&TimedMeasurement> {
        self.measures.as_ref().and_then(|measurements| {
            let mut sorted_set: Vec<&TimedMeasurement> = measurements.iter().collect();
            sorted_set.sort_by(|a,b| a.time_stamp.cmp(&b.time_stamp));
            return sorted_set.last().copied();
        })
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TimedMeasurement {
    pub time_stamp: DateTime<Utc>,
    #[serde(flatten)]
    pub value: Measurement,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(tag = "unit", content = "value")]
pub enum Measurement {
    #[serde(rename = "C")]
    #[serde(deserialize_with = "deserialize_number_from_string")]
    Celsius(f32),
    #[serde(rename = "F")]
    #[serde(deserialize_with = "deserialize_number_from_string")]
    Fahrenheit(f32),
    #[serde(rename = "%")]
    #[serde(deserialize_with = "deserialize_number_from_string")]
    Percentage(f32),
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub enum MeasurementUnit {
    #[serde(rename = "C")]
    Celsius,
    #[serde(rename = "F")]
    Fahrenheit,
    #[serde(rename = "%")]
    Percentage,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct PlantMinimalDetails {
    pub id: String,
    pub module: ModuleMinimalDetail,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
pub struct ModuleMinimalDetail {
    pub id: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SetStatusRequest {
    pub function: ThermostatFunction,
    pub mode: ThermostatMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_point: Option<Measurement>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub programs: Option<Vec<ProgramIdentifier>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activation_time: Option<String>,
}

impl SetStatusRequest {
    pub fn validate(&self) -> bool {
        match self.mode {
            ThermostatMode::Manual => {
                self.set_point.is_some()
            }
            ThermostatMode::Boost => {
                self.activation_time.is_some()
            },
            ThermostatMode::Automatic => {
                self.programs.is_some()
            },
            _ => true
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionInfo {
    pub subscription_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "EndPointUrl")]
    pub endpoint_url: Option<String>
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct C2CEvent {
    pub id: String,
    pub event_type: String,
    pub subject: String,
    pub event_time: DateTime<Utc>,
    pub data: ModuleStatus,
    #[serde(flatten)]
    pub subject_elements: Option<HashMap<String, serde_json::Value>>
}

pub type C2CEvents = Vec<C2CEvent>;
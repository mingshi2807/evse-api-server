use serde::{Deserialize, Serialize};

// ── Client → Server commands ──────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Command {
    #[serde(rename = "configure")]
    Configure { config: EvseConfig },

    #[serde(rename = "start")]
    Start {
        interface: String,
        tls: bool,
        #[serde(default = "default_port")]
        port: u16,
    },

    #[serde(rename = "stop")]
    Stop,

    #[serde(rename = "control_event")]
    ControlEvent {
        session_id: String,
        event: ControlEventPayload,
    },

    #[serde(rename = "subscribe")]
    Subscribe { categories: Vec<String> },
}

fn default_port() -> u16 {
    50000
}

#[derive(Debug, Deserialize)]
pub struct EvseConfig {
    pub evse_id: String,
    pub energy_services: Vec<String>,
    pub auth_services: Vec<String>,
    #[serde(default)]
    pub vas_services: Vec<u16>,
    #[serde(default)]
    pub cert_install: bool,
    pub control_mode: String,
    pub mobility_mode: String,
    #[serde(default)]
    pub control_mobility_modes: Vec<ControlMobilityMode>,
    pub dc_limits: Option<DcLimitsConfig>,
    pub ac_limits: Option<AcLimitsConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ControlMobilityMode {
    pub control_mode: String,
    pub mobility_mode: String,
}

#[derive(Debug, Deserialize)]
pub struct DcLimitsConfig {
    pub max_voltage: f64,
    pub max_current: f64,
    pub max_power: f64,
    #[serde(default)]
    pub min_power: f64,
    pub discharge_limits: Option<Box<DcLimitsConfig>>,
}

#[derive(Debug, Deserialize)]
pub struct AcLimitsConfig {
    pub max_charge_power: f64,
    #[serde(default)]
    pub min_charge_power: f64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ControlEventPayload {
    #[serde(rename = "AuthorizationResponse")]
    AuthorizationResponse { authorized: bool },

    #[serde(rename = "CableCheckFinished")]
    CableCheckFinished { success: bool },

    #[serde(rename = "PresentVoltageCurrent")]
    PresentVoltageCurrent { voltage: f64, current: f64 },

    #[serde(rename = "StopCharging")]
    StopCharging { stop: bool },

    #[serde(rename = "PauseCharging")]
    PauseCharging { pause: bool },

    #[serde(rename = "ClosedContactor")]
    ClosedContactor { closed: bool },

    #[serde(rename = "DcTransferLimits")]
    DcTransferLimits {
        charge_limits: PowerCurrentLimits,
        voltage: MinMax,
        #[serde(default)]
        discharge_limits: Option<PowerCurrentLimits>,
    },

    #[serde(rename = "AcTransferLimits")]
    AcTransferLimits { charge_power: MinMax },

    #[serde(rename = "UpdateDynamicModeParameters")]
    UpdateDynamicModeParameters {
        #[serde(default)]
        departure_time: u64,
        #[serde(default)]
        target_soc: u8,
        #[serde(default)]
        min_soc: u8,
    },

    #[serde(rename = "AcTargetPower")]
    AcTargetPower { target_active_power: f64 },

    #[serde(rename = "AcPresentPower")]
    AcPresentPower { present_active_power: f64 },

    #[serde(rename = "EnergyServices")]
    EnergyServices { services: Vec<String> },

    #[serde(rename = "SupportedVASs")]
    SupportedVASs { service_ids: Vec<u16> },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PowerCurrentLimits {
    pub power: MinMax,
    pub current: CurrentMax,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MinMax {
    pub max: f64,
    #[serde(default)]
    pub min: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CurrentMax {
    pub max: f64,
}

// ── Server → Client events ────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ServerEvent {
    #[serde(rename = "signal")]
    Signal { session_id: String, signal: String },

    #[serde(rename = "state_change")]
    StateChange { session_id: String, state: String },

    #[serde(rename = "v2g_message")]
    V2gMessage {
        session_id: String,
        msg_type: String,
    },

    #[serde(rename = "evcc_id")]
    EvccId { session_id: String, evcc_id: String },

    #[serde(rename = "selected_protocol")]
    SelectedProtocol {
        session_id: String,
        protocol: String,
    },

    #[serde(rename = "selected_service")]
    SelectedService {
        session_id: String,
        energy_service: String,
        control_mode: String,
        mobility_mode: String,
        pricing: String,
    },

    #[serde(rename = "error")]
    Error {
        session_id: String,
        code: String,
        message: String,
    },

    #[serde(rename = "status")]
    Status {
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        port: Option<u16>,
        #[serde(skip_serializing_if = "Option::is_none")]
        interface: Option<String>,
    },

    #[serde(rename = "session_closed")]
    SessionClosed { session_id: String, reason: String },
}

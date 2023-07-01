#[derive(serde::Deserialize, Clone)]
pub struct ApplicationConfig {
    pub redis_connection_string: String,
    #[serde(default)]
    pub worker_api_base: String,
    pub realtime_api_key: String,
    pub reference_data_api_key: String,
}

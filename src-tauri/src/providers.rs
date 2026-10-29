//! Extension boundaries only: Alpha does not instantiate providers or issue requests.

pub struct SensorReading {
    pub name: String,
    pub value: f64,
    pub unit: String,
}

pub trait SensorProvider: Send {
    fn readings(&mut self) -> Result<Vec<SensorReading>, String>;
}

pub trait OllamaProvider: Send {
    fn analyze(&self, snapshot: &crate::model::Snapshot) -> Result<String, String>;
}

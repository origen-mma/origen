use rdkafka::{
    consumer::{Consumer, StreamConsumer},
    error::KafkaError,
    ClientConfig, Message,
};
use thiserror::Error;
use tracing::{debug, error, info};

#[derive(Debug, Error)]
pub enum BoomConsumerError {
    #[error("Kafka error: {0}")]
    Kafka(#[from] KafkaError),

    #[error("Message decode error: {0}")]
    DecodeError(String),
}

#[derive(Debug, Clone)]
pub struct BoomConsumerConfig {
    pub bootstrap_servers: String,
    pub sasl_username: String,
    pub sasl_password: String,
    pub group_id: String,
    pub topics: Vec<String>,
}

impl Default for BoomConsumerConfig {
    fn default() -> Self {
        Self {
            bootstrap_servers: "kaboom.caltech.edu:9093".to_string(),
            sasl_username: "CHANGE_ME".to_string(),
            sasl_password: "CHANGE_ME".to_string(),
            group_id: "origin".to_string(),
            topics: vec![
                // ZTF topics
                "babamul.ztf.no-lsst-match.stellar".to_string(),
                "babamul.ztf.no-lsst-match.hosted".to_string(),
                "babamul.ztf.no-lsst-match.hostless".to_string(),
                "babamul.ztf.ztfbh-partnership.stellar".to_string(),
                "babamul.ztf.ztfbh-partnership.hosted".to_string(),
                "babamul.ztf.ztfbh-partnership.hostless".to_string(),
                "babamul.ztf.ztf-partnership.stellar".to_string(),
                "babamul.ztf.ztf-partnership.hosted".to_string(),
                "babamul.ztf.ztf-partnership.hostless".to_string(),
                // LSST topics (future)
                "babamul.lsst.no-ztf-match.stellar".to_string(),
                "babamul.lsst.no-ztf-match.hosted".to_string(),
                "babamul.lsst.no-ztf-match.hostless".to_string(),
                "babamul.lsst.lsst-partnership.stellar".to_string(),
                "babamul.lsst.lsst-partnership.hosted".to_string(),
            ],
        }
    }
}

pub struct BoomConsumer {
    consumer: StreamConsumer,
}

impl BoomConsumer {
    /// Create a new BOOM Kafka consumer
    pub fn new(config: BoomConsumerConfig) -> Result<Self, BoomConsumerError> {
        info!("Creating BOOM consumer: {}", config.bootstrap_servers);

        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", &config.bootstrap_servers)
            .set("security.protocol", "SASL_PLAINTEXT")
            .set("sasl.mechanisms", "SCRAM-SHA-512")
            .set("sasl.username", &config.sasl_username)
            .set("sasl.password", &config.sasl_password)
            .set("group.id", &config.group_id)
            .set("enable.auto.commit", "true")
            .set("auto.offset.reset", "latest")
            .create()?;

        // Subscribe to topics
        let topic_refs: Vec<&str> = config.topics.iter().map(|s| s.as_str()).collect();
        consumer.subscribe(&topic_refs)?;

        info!("Subscribed to {} BOOM topics", config.topics.len());
        for topic in &config.topics {
            debug!("  - {}", topic);
        }

        Ok(Self { consumer })
    }

    /// Get the next message from Kafka
    pub async fn recv(&self) -> Result<Option<Vec<u8>>, BoomConsumerError> {
        match self.consumer.recv().await {
            Ok(msg) => {
                if let Some(payload) = msg.payload() {
                    Ok(Some(payload.to_vec()))
                } else {
                    Ok(None)
                }
            }
            Err(e) => {
                error!("Error receiving message: {}", e);
                Err(BoomConsumerError::Kafka(e))
            }
        }
    }

    /// Stream messages continuously
    pub async fn stream<F>(&self, mut handler: F) -> Result<(), BoomConsumerError>
    where
        F: FnMut(Vec<u8>) -> Result<(), Box<dyn std::error::Error>>,
    {
        loop {
            match self.recv().await {
                Ok(Some(payload)) => {
                    if let Err(e) = handler(payload) {
                        error!("Handler error: {}", e);
                    }
                }
                Ok(None) => {
                    debug!("Received empty message");
                }
                Err(e) => {
                    error!("Stream error: {}", e);
                    return Err(e);
                }
            }
        }
    }
}

//! Encrypted MQTT client wrapper providing defense-in-depth payload encryption.

use std::sync::Arc;
use anyhow::{Context, Result};
use nebula_core::security::envelope::SecurityEnvelope;
use nebula_core::security::keys::{AesKey, HmacKey};
use super::client::{MessageHandler, MqttClient};

/// MQTT client wrapper that encrypts/decrypts payloads using the NEBULA SecurityEnvelope.
pub struct EncryptedMqttClient {
    inner: MqttClient,
    hmac_key: HmacKey,
    aes_key: AesKey,
}

impl EncryptedMqttClient {
    pub fn new(inner: MqttClient, hmac_key: HmacKey, aes_key: AesKey) -> Self {
        Self { inner, hmac_key, aes_key }
    }

    pub fn client_id(&self) -> &str { self.inner.client_id() }
    pub fn inner(&self) -> &MqttClient { &self.inner }
    pub fn inner_mut(&mut self) -> &mut MqttClient { &mut self.inner }

    pub async fn publish_encrypted(&self, topic: &str, plaintext: &[u8]) -> Result<()> {
        let ciphertext = SecurityEnvelope::seal(plaintext, &self.hmac_key, &self.aes_key)
            .context("Failed to encrypt MQTT payload")?;
        self.inner.publish(topic, &ciphertext).await
    }

    pub fn on_encrypted_message<F>(&self, topic_filter: &str, handler: F)
    where F: Fn(String, Vec<u8>) + Send + Sync + 'static,
    {
        let hmac_key = self.hmac_key.clone();
        let aes_key = self.aes_key.clone();
        let decrypting_handler: MessageHandler = Arc::new(move |topic, payload| {
            if !SecurityEnvelope::verify_hmac(&payload, &hmac_key) {
                tracing::warn!(topic = %topic, "Dropping MQTT message: HMAC verification failed");
                return;
            }
            match SecurityEnvelope::open(&payload, &hmac_key, &aes_key) {
                Ok(plaintext) => handler(topic, plaintext),
                Err(e) => tracing::warn!(topic = %topic, error = %e, "Dropping MQTT message: decryption failed"),
            }
        });
        self.inner.on_message(topic_filter, decrypting_handler);
    }

    pub async fn subscribe(&self, topic: &str) -> Result<()> { self.inner.subscribe(topic).await }
    pub async fn connect(&mut self) -> Result<()> { self.inner.connect().await }
    pub async fn disconnect(&self) -> Result<()> { self.inner.disconnect().await }
    pub fn message_receiver(&mut self) -> Option<tokio::sync::mpsc::Receiver<(String, Vec<u8>)>> {
        self.inner.message_receiver()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nebula_core::security::keys::KeyPair;

    #[test]
    fn test_encrypted_client_creation() {
        let inner = MqttClient::new("test-node", "localhost", 1883, None).unwrap();
        let keys = KeyPair::derive_from_secret(b"test-secret").unwrap();
        let client = EncryptedMqttClient::new(inner, keys.hmac, keys.aes);
        assert_eq!(client.client_id(), "test-node");
    }

    #[test]
    fn test_encrypted_client_inner_access() {
        let inner = MqttClient::new("test-node", "localhost", 1883, None).unwrap();
        let keys = KeyPair::derive_from_secret(b"test-secret").unwrap();
        let client = EncryptedMqttClient::new(inner, keys.hmac, keys.aes);
        assert_eq!(client.inner().client_id(), "test-node");
    }

    #[test]
    fn test_seal_open_roundtrip_foundation() {
        let keys = KeyPair::derive_from_secret(b"mqtt-test").unwrap();
        let plaintext = b"Hello, encrypted MQTT!";
        let sealed = SecurityEnvelope::seal(plaintext, &keys.hmac, &keys.aes).unwrap();
        let opened = SecurityEnvelope::open(&sealed, &keys.hmac, &keys.aes).unwrap();
        assert_eq!(opened, plaintext);
    }

    #[test]
    fn test_handler_registration() {
        let inner = MqttClient::new("test-node", "localhost", 1883, None).unwrap();
        let keys = KeyPair::derive_from_secret(b"test-secret").unwrap();
        let client = EncryptedMqttClient::new(inner, keys.hmac, keys.aes);
        client.on_encrypted_message("test/#", |_topic, _payload| {});
        let handlers = client.inner().handlers().read().unwrap();
        assert_eq!(handlers.len(), 1);
        assert!(handlers.contains_key("test/#"));
    }

    #[test]
    fn test_message_receiver_returns_some_once() {
        let inner = MqttClient::new("test-node", "localhost", 1883, None).unwrap();
        let keys = KeyPair::derive_from_secret(b"test-secret").unwrap();
        let mut client = EncryptedMqttClient::new(inner, keys.hmac, keys.aes);
        assert!(client.message_receiver().is_some());
        assert!(client.message_receiver().is_none());
    }
}

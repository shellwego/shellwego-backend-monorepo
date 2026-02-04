//! KMS provider implementations

use async_trait::async_trait;

use crate::kms::{KmsProvider, KmsError, DataKey, MasterKey};

/// HashiCorp Vault provider
pub struct VaultProvider {
    // TODO: Add client, mount_point, token
}

#[async_trait]
impl KmsProvider for VaultProvider {
    // TODO: impl encrypt via Vault transit API
    // TODO: impl decrypt via Vault transit API
    // TODO: impl generate_data_key
    // TODO: impl rotate_key
    // TODO: impl health_check
}

/// AWS KMS provider
pub struct AwsKmsProvider {
    // TODO: Add aws_sdk_kms client, key_id
}

#[async_trait]
impl KmsProvider for AwsKmsProvider {
    // TODO: impl using aws_sdk_kms
}

/// GCP Cloud KMS provider
pub struct GcpKmsProvider {
    // TODO: Add google_cloud_kms client
}

#[async_trait]
impl KmsProvider for GcpKmsProvider {
    // TODO: impl using google-cloud-kms
}

/// Azure Key Vault provider
pub struct AzureKeyVaultProvider {
    // TODO: Add azure_security_keyvault client
}

#[async_trait]
impl KmsProvider for AzureKeyVaultProvider {
    // TODO: impl using azure_security_keyvault
}

/// File-based provider (development only)
pub struct FileProvider {
    // TODO: Add key_file_path, passphrase
}

#[async_trait]
impl KmsProvider for FileProvider {
    // TODO: impl using age or PGP encryption
    // TODO: WARN: Not for production use
}
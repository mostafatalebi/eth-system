use serde::Deserialize;
use validator::Validate;

#[derive(Deserialize)]

pub struct ProductModel {
    id: u64,
    title: String,
    price: f64,
    sku: String,
    brand_id: u64,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ProductRequestModel {
    #[validate(length(min = 1, max = 128))]
    title: String,
    #[validate(range(min = 0.0, max = 1_000_000_000.0))]
    price: f64,
    #[validate(length(equal=8))]
    sku: String,
    #[validate(range(min=1))]
    brand_id: u64
}


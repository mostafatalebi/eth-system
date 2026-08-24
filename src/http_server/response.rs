use std::collections::HashMap;
use axum::http::{StatusCode};
use axum::Json;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ApiResponse<T> {
    #[serde(skip_serializing, skip_deserializing)]
    status_code: StatusCode,
    ok: bool,
    error: Option<ErrorResponse>,
    data: Option<T>,
}

impl<T: Serialize> Default for ApiResponse<T> {
    fn default() -> Self {
        Self {
            status_code: StatusCode::OK,
            ok: true,
            error: None,
            data: None,
        }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> axum::response::Response {
        (
            self.status_code,
            Json(self)
        )
        .into_response()
    }
}

impl<T: Serialize> ApiResponse<T> {
    pub fn for_success(data: Option<T>) -> Self {
        Self {
            status_code: StatusCode::OK,
            ok: true,
            error: None,
            data,
        }
    }

    pub fn for_error(err_msg: String) -> Self {
        Self {
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
            ok: false,
            error: Some(ErrorResponse::new(err_msg)),
            data: None,
        }
    }

    pub fn with_error(err: ErrorResponse) -> Self {
        Self {
            status_code: StatusCode::INTERNAL_SERVER_ERROR,
            ok: false,
            error: Some(err),
            data: None,
        }
    }

    pub fn set_error(&mut self, err: ErrorResponse) {
        self.ok = false;
        self.error = Some(err);
    }
    pub fn set_status_code(&mut self, status_code: StatusCode) {
        self.status_code = status_code;
    }
    pub fn set_data(&mut self, data: T) {
        self.data = Some(data);
    }
    pub fn set_ok(&mut self, ok: bool) {
        self.ok = ok;
    }


}


#[derive(Deserialize, Serialize)]
pub struct ErrorResponse {
    error: String,
    fields: Option<HashMap<String, String>>,
}

impl ErrorResponse {
    pub fn new(error: String) -> Self {
        Self {
            error,
            fields: None,
        }
    }

    pub fn add_field_error(&mut self, field: String, err: String) {
        if self.fields.is_none() {
            self.fields = Some(HashMap::new());
        }

        _ = self.fields.as_mut().unwrap().insert(field, err);
    }
}
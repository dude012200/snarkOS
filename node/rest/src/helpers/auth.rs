// Copyright (C) 2019-2023 Aleo Systems Inc.
// This file is part of the snarkOS library.

// The snarkOS library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkOS library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkOS library. If not, see <https://www.gnu.org/licenses/>.

use snarkvm::prelude::*;

use ::time::OffsetDateTime;
use anyhow::{anyhow, Result};
use axum::{
    headers::authorization::{Authorization, Bearer},
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    RequestPartsExt,
    TypedHeader,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

/// The time a jwt token is valid for.
pub const EXPIRATION: i64 = 10 * 365 * 24 * 60 * 60; // 10 years.

/// Returns the JWT secret for the node instance.
fn jwt_secret() -> &'static Vec<u8> {
    static SECRET: OnceCell<Vec<u8>> = OnceCell::new();
    SECRET.get_or_init(|| {
        let seed: [u8; 16] = ::rand::thread_rng().gen();
        seed.to_vec()
    })
}

/// The Json web token claims.
#[derive(Debug, Deserialize, Serialize)]
pub struct Claims {
    /// The subject (user).
    sub: String,
    /// The UTC timestamp the token was issued at.
    iat: i64,
    /// Expiration time (as UTC timestamp).
    exp: i64,
}

impl Claims {
    pub fn new<N: Network>(address: Address<N>) -> Self {
        let issued_at = OffsetDateTime::now_utc().unix_timestamp();
        let expiration = issued_at.saturating_add(EXPIRATION);

        Self { sub: address.to_string(), iat: issued_at, exp: expiration }
    }

    /// Returns true if the token is expired.
    pub fn is_expired(&self) -> bool {
        OffsetDateTime::now_utc().unix_timestamp() >= self.exp
    }

    /// Returns the json web token string.
    pub fn to_jwt_string(&self) -> Result<String> {
        encode(&Header::default(), &self, &EncodingKey::from_secret(jwt_secret())).map_err(|e| anyhow!(e))
    }
}

pub async fn auth_middleware<B>(request: Request<B>, next: Next<B>) -> Result<Response, Response>
where
    B: Send,
{
    // Deconstruct the request to extract the auth token.
    let (mut parts, body) = request.into_parts();
    let auth: TypedHeader<Authorization<Bearer>> =
        parts.extract().await.map_err(|_| StatusCode::UNAUTHORIZED.into_response())?;

    match decode::<Claims>(auth.token(), &DecodingKey::from_secret(jwt_secret()), &Validation::new(Algorithm::HS256)) {
        Ok(decoded) => {
            let claims = decoded.claims;
            if claims.is_expired() {
                return Err((StatusCode::UNAUTHORIZED, "Expired JSON Web Token".to_owned()).into_response());
            }
        }

        Err(_) => {
            return Err(StatusCode::UNAUTHORIZED.into_response());
        }
    }

    // Reconstruct the request.
    let request = Request::from_parts(parts, body);

    Ok(next.run(request).await)
}

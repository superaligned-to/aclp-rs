use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use jsonschema::Validator;
use serde_json::Value;

use crate::json;
use crate::local_schema::{self, SchemaError};

/// An installed Draft-07 result schema for request callbacks returning `T`.
///
/// The schema is compiled once; clones share the compiled validator. Server and client use the
/// same instance so a result is checked against one contract on both sides. `T` is the typed view
/// of a valid result; the schema, not `T`, decides what is valid.
pub struct ResultSchema<T> {
    schema: Arc<Value>,
    validator: Arc<Validator>,
    ty: PhantomData<fn() -> T>,
}

impl<T> ResultSchema<T> {
    /// Compiles a result schema.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError`] when the schema exceeds the complexity ceiling, declares a dialect
    /// other than Draft-07, is invalid, or references a resource outside the document.
    pub fn new(schema: Value) -> Result<Self, SchemaError> {
        let validator = local_schema::compile(&schema)?;

        Ok(ResultSchema {
            schema: Arc::new(schema),
            validator: Arc::new(validator),
            ty: PhantomData,
        })
    }

    /// The installed schema document.
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    /// Reports whether `val` is a bounded JSON value the schema accepts, without coercion or defaults.
    pub fn is_valid(&self, val: &Value) -> bool {
        json::check_bounds(val).is_ok() && self.validator.is_valid(val)
    }

    pub(crate) fn erase(&self) -> ResultSchema<Value> {
        ResultSchema {
            schema: Arc::clone(&self.schema),
            validator: Arc::clone(&self.validator),
            ty: PhantomData,
        }
    }
}

impl<T> Clone for ResultSchema<T> {
    fn clone(&self) -> Self {
        ResultSchema {
            schema: Arc::clone(&self.schema),
            validator: Arc::clone(&self.validator),
            ty: PhantomData,
        }
    }
}

impl<T> fmt::Debug for ResultSchema<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResultSchema")
            .field("schema", &self.schema)
            .finish_non_exhaustive()
    }
}

use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::Review;
use crate::ports::{ReviewStore, StoreError};

/// shortcut: reviews live in process memory and die with it, and lookups scan
/// the list; add a database adapter of `ReviewStore` when reviews must survive
/// a restart or grow past a few hundred.
#[derive(Default)]
pub struct MemoryStore(Mutex<Vec<Review>>);

#[async_trait]
impl ReviewStore for MemoryStore {
    async fn get(&self, id: Uuid) -> Result<Option<Review>, StoreError> {
        let reviews = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(reviews.iter().find(|r| r.id == id).cloned())
    }

    async fn put(&self, review: Review) -> Result<(), StoreError> {
        let mut reviews = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        match reviews.iter_mut().find(|r| r.id == review.id) {
            Some(stored) => *stored = review,
            None => reviews.push(review),
        }
        Ok(())
    }

    async fn list(&self) -> Result<Vec<Review>, StoreError> {
        let reviews = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(reviews.iter().rev().cloned().collect())
    }
}

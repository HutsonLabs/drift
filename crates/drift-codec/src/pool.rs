//! The rayon tile pool shared by the CPU codecs.

use std::sync::Arc;

/// Upper bound on worker threads: a 1280×800 progressive refresh is 260 tiles, and more
/// workers than this only add scheduling overhead next to the render and network threads.
const MAX_DEFAULT_THREADS: usize = 8;

/// A dedicated rayon thread pool for per-tile decode work.
///
/// Cloning is cheap (the pool is shared). One pool is meant to serve every session: tile
/// jobs are short and CPU bound, so sharing avoids oversubscribing the machine when several
/// tabs decode at once.
#[derive(Clone)]
pub struct TilePool {
    pool: Arc<rayon::ThreadPool>,
}

impl std::fmt::Debug for TilePool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TilePool").field("threads", &self.threads()).finish()
    }
}

impl TilePool {
    /// Creates a pool with `threads` workers (at least one).
    ///
    /// # Errors
    /// Returns the rayon error if the OS refuses to spawn the worker threads.
    pub fn new(threads: usize) -> Result<Self, rayon::ThreadPoolBuildError> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads.max(1))
            .thread_name(|i| format!("drift-codec-{i}"))
            .build()?;
        Ok(Self { pool: Arc::new(pool) })
    }

    /// Creates a pool sized to the machine: `available_parallelism`, capped at 8.
    ///
    /// # Errors
    /// Returns the rayon error if the OS refuses to spawn the worker threads.
    pub fn with_default_threads() -> Result<Self, rayon::ThreadPoolBuildError> {
        Self::new(default_threads())
    }

    /// Number of worker threads.
    pub fn threads(&self) -> usize {
        self.pool.current_num_threads()
    }

    /// Runs `f` inside the pool, so rayon parallel iterators in `f` use its workers.
    pub(crate) fn install<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
        self.pool.install(f)
    }
}

/// Default worker count for [`TilePool::with_default_threads`].
pub(crate) fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(1, |n| n.get()).clamp(1, MAX_DEFAULT_THREADS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_has_the_requested_threads_and_at_least_one() {
        assert_eq!(TilePool::new(3).map(|p| p.threads()).ok(), Some(3));
        assert_eq!(TilePool::new(0).map(|p| p.threads()).ok(), Some(1));
    }

    #[test]
    fn default_pool_is_capped() {
        let pool = TilePool::with_default_threads().ok();
        let threads = pool.as_ref().map(TilePool::threads).unwrap_or_default();
        assert!((1..=MAX_DEFAULT_THREADS).contains(&threads));
        assert_eq!(threads, default_threads());
        assert!(format!("{pool:?}").contains("threads"));
    }

    #[test]
    fn install_runs_on_pool_workers() {
        let pool = TilePool::new(2).ok();
        let idx = pool.map(|p| p.install(rayon::current_thread_index));
        assert!(matches!(idx, Some(Some(0 | 1))));
    }
}

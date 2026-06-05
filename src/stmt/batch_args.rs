//! SQL batch statement arguments — the `ToBatchSql` trait and implementations.

use super::batch_bind::BatchParams;
use crate::{oci::*, Result, Error};
use std::mem::size_of;
use libc::c_void;

/// A trait for types that can be used as batch SQL arguments.
///
/// Each implementation binds an **array** of values (one per row in the batch)
/// to a single SQL parameter placeholder. This is used with
/// [`Statement::execute_batch`](crate::Statement::execute_batch).
///
/// # Example
///
/// ```rust,no_run
/// # #[cfg(feature="blocking")]
/// # fn main() -> sibyl::Result<()> {
/// # let session = sibyl::test_env::get_session()?;
/// let stmt = session.prepare("INSERT INTO hr.regions (region_id, region_name) VALUES (:id, :name)")?;
/// let ids:   &[i32] = &[100, 101, 102];
/// let names: &[&str] = &["North", "South", "East"];
/// let rows = stmt.execute_batch(3, ((":ID", ids), (":NAME", names)))?;
/// assert_eq!(rows, 3);
/// # session.rollback()?;
/// # Ok(())
/// # }
/// # #[cfg(feature="nonblocking")]
/// # fn main() {}
/// ```
pub trait ToBatchSql: Send + Sync {
    /// Binds a batch of values to SQL parameter placeholder(s).
    ///
    /// `pos` is the zero-based index of the first parameter to bind.
    /// `batch_size` is the number of rows in the batch.
    ///
    /// Returns the index of the placeholder for the next argument.
    fn bind_batch_to(
        &mut self,
        pos: usize,
        batch_size: usize,
        params: &mut BatchParams,
        stmt: &OCIStmt,
        err: &OCIError,
    ) -> Result<usize>;

    /// Called after batch execution to update OUT (or INOUT) arguments.
    ///
    /// Default implementation does nothing (for IN-only args).
    fn update_batch_from_bind(
        &mut self,
        pos: usize,
        _batch_size: usize,
        _params: &BatchParams,
    ) -> Result<usize> {
        Ok(pos + 1)
    }
}

// ─── Numeric IN slices ────────────────────────────────────────────────────────

macro_rules! impl_num_batch_in {
    ($($t:ty),+ => $sqlt:ident) => {
        $(
            /// Batch-bind a slice of fixed-size values as IN parameters.
            impl ToBatchSql for &[$t] {
                fn bind_batch_to(&mut self, pos: usize, batch_size: usize, params: &mut BatchParams, stmt: &OCIStmt, err: &OCIError) -> Result<usize> {
                    params.check_batch_len(self.len(), stringify!($t))?;
                    params.bind_batch_in_fixed(
                        pos, $sqlt,
                        self.as_ptr() as *const c_void,
                        size_of::<$t>(),
                        stmt, err,
                    )?;
                    let _ = batch_size;
                    Ok(pos + 1)
                }
            }
        )+
    };
}

impl_num_batch_in! { i8, i16, i32, i64, isize => SQLT_INT }
impl_num_batch_in! { u16, u32, u64, usize => SQLT_UIN }
impl_num_batch_in! { f32 => SQLT_BFLOAT }
impl_num_batch_in! { f64 => SQLT_BDOUBLE }

// u8 as byte (SQLT_UIN) — not &[u8] raw binary
impl ToBatchSql for &[u8] {
    fn bind_batch_to(&mut self, pos: usize, batch_size: usize, params: &mut BatchParams, stmt: &OCIStmt, err: &OCIError) -> Result<usize> {
        params.check_batch_len(self.len(), "u8")?;
        params.bind_batch_in_fixed(
            pos, SQLT_UIN,
            self.as_ptr() as *const c_void,
            size_of::<u8>(),
            stmt, err,
        )?;
        let _ = batch_size;
        Ok(pos + 1)
    }
}

// ─── Numeric OUT mutable slices ───────────────────────────────────────────────

macro_rules! impl_num_batch_out {
    ($($t:ty),+ => $sqlt:ident) => {
        $(
            /// Batch-bind a mutable slice of fixed-size values as OUT parameters.
            /// OCI writes results directly into the slice memory.
            impl ToBatchSql for &mut [$t] {
                fn bind_batch_to(&mut self, pos: usize, batch_size: usize, params: &mut BatchParams, stmt: &OCIStmt, err: &OCIError) -> Result<usize> {
                    params.check_batch_len(self.len(), stringify!($t))?;
                    let _ = batch_size;
                    params.bind_batch_out_fixed(
                        pos, $sqlt,
                        self.as_mut_ptr() as *mut c_void,
                        size_of::<$t>(),
                        stmt, err,
                    )?;
                    Ok(pos + 1)
                }
                // No update_batch_from_bind needed: OCI writes directly into slice memory.
            }
        )+
    };
}

impl_num_batch_out! { i8, i16, i32, i64, isize => SQLT_INT }
impl_num_batch_out! { u8, u16, u32, u64, usize => SQLT_UIN }
impl_num_batch_out! { f32 => SQLT_BFLOAT }
impl_num_batch_out! { f64 => SQLT_BDOUBLE }

// ─── Nullable numeric IN slices ───────────────────────────────────────────────

macro_rules! impl_option_num_batch_in {
    ($($t:ty),+ => $sqlt:ident) => {
        $(
            /// Batch-bind a slice of optional fixed-size values.
            /// `None` entries become NULL in the database.
            impl ToBatchSql for &[Option<$t>] {
                fn bind_batch_to(&mut self, pos: usize, batch_size: usize, params: &mut BatchParams, stmt: &OCIStmt, err: &OCIError) -> Result<usize> {
                    params.check_batch_len(self.len(), stringify!(Option<$t>))?;
                    let ptrs: Vec<Option<*const c_void>> = self.iter()
                        .map(|opt| opt.as_ref().map(|v| v as *const $t as *const c_void))
                        .collect();
                    params.bind_batch_nullable_fixed(pos, $sqlt, size_of::<$t>(), &ptrs, stmt, err)?;
                    let _ = batch_size;
                    Ok(pos + 1)
                }
            }
        )+
    };
}

impl_option_num_batch_in! { i8, i16, i32, i64, isize => SQLT_INT }
impl_option_num_batch_in! { u8, u16, u32, u64, usize => SQLT_UIN }
impl_option_num_batch_in! { f32 => SQLT_BFLOAT }
impl_option_num_batch_in! { f64 => SQLT_BDOUBLE }

// ─── Nullable numeric OUT mutable slices ─────────────────────────────────────

macro_rules! impl_option_num_batch_out {
    ($($t:ty),+ => $sqlt:ident) => {
        $(
            /// Batch-bind a mutable slice of optional fixed-size values as OUT parameters.
            /// After execution, each `None` element is set to `None` if the row was NULL;
            /// `Some` elements retain OCI-written values.
            impl ToBatchSql for &mut [Option<$t>] {
                fn bind_batch_to(&mut self, pos: usize, batch_size: usize, params: &mut BatchParams, stmt: &OCIStmt, err: &OCIError) -> Result<usize> {
                    params.check_batch_len(self.len(), stringify!(Option<$t>))?;
                    let _ = batch_size;
                    let mut vec = Vec::with_capacity(self.len());
                    for opt in self.iter() {
                        vec.push(opt.unwrap_or_default());
                    }
                    params.bind_batch_nullable_out_fixed(pos, $sqlt, size_of::<$t>(), vec.as_ptr() as *const c_void, stmt, err)?;
                    Ok(pos + 1)
                }

                fn update_batch_from_bind(&mut self, pos: usize, batch_size: usize, params: &BatchParams) -> Result<usize> {
                    for i in 0..batch_size {
                        if params.is_null_at(pos, i) {
                            self[i] = None;
                        } else if let Some(val) = params.get_fixed_data_at::<$t>(pos, i) {
                            self[i] = Some(val);
                        }
                    }
                    Ok(pos + 1)
                }
            }
        )+
    };
}

impl_option_num_batch_out! { i8, i16, i32, i64, isize => SQLT_INT }
impl_option_num_batch_out! { u8, u16, u32, u64, usize => SQLT_UIN }
impl_option_num_batch_out! { f32 => SQLT_BFLOAT }
impl_option_num_batch_out! { f64 => SQLT_BDOUBLE }

// ─── String IN slices ─────────────────────────────────────────────────────────

/// Batch-bind a slice of `&str` values as IN parameters.
/// Strings are packed into an internal padded buffer (max-length stride).
impl ToBatchSql for &[&str] {
    fn bind_batch_to(&mut self, pos: usize, batch_size: usize, params: &mut BatchParams, stmt: &OCIStmt, err: &OCIError) -> Result<usize> {
        params.check_batch_len(self.len(), "&str")?;
        let strings: Vec<(*const u8, usize)> = self.iter().map(|s| (s.as_ptr(), s.len())).collect();
        params.bind_batch_str_in(pos, &strings, stmt, err)?;
        let _ = batch_size;
        Ok(pos + 1)
    }
}

/// Batch-bind a slice of `String` values as IN parameters.
impl ToBatchSql for &[String] {
    fn bind_batch_to(&mut self, pos: usize, batch_size: usize, params: &mut BatchParams, stmt: &OCIStmt, err: &OCIError) -> Result<usize> {
        params.check_batch_len(self.len(), "String")?;
        let strings: Vec<(*const u8, usize)> = self.iter().map(|s| (s.as_ptr(), s.len())).collect();
        params.bind_batch_str_in(pos, &strings, stmt, err)?;
        let _ = batch_size;
        Ok(pos + 1)
    }
}

/// Batch-bind a slice of optional `&str` values as IN parameters.
/// `None` entries become NULL.
impl ToBatchSql for &[Option<&str>] {
    fn bind_batch_to(&mut self, pos: usize, batch_size: usize, params: &mut BatchParams, stmt: &OCIStmt, err: &OCIError) -> Result<usize> {
        params.check_batch_len(self.len(), "Option<&str>")?;
        let strings: Vec<(*const u8, usize)> = self.iter().map(|opt| {
            opt.map(|s| (s.as_ptr(), s.len())).unwrap_or((std::ptr::null(), 0))
        }).collect();
        params.bind_batch_str_in(pos, &strings, stmt, err)?;
        let _ = batch_size;
        Ok(pos + 1)
    }
}

/// Batch-bind a mutable slice of `String` values as OUT parameters.
/// Each string must have sufficient capacity. OCI writes into an internal
/// buffer and the results are copied back in `update_batch_from_bind`.
impl ToBatchSql for &mut [String] {
    fn bind_batch_to(&mut self, pos: usize, batch_size: usize, params: &mut BatchParams, stmt: &OCIStmt, err: &OCIError) -> Result<usize> {
        params.check_batch_len(self.len(), "String (OUT)")?;
        let max_cap = self.iter().map(|s| s.capacity()).max().unwrap_or(256).max(1);
        params.bind_batch_str_out(pos, max_cap, stmt, err)?;
        let _ = batch_size;
        Ok(pos + 1)
    }

    fn update_batch_from_bind(&mut self, pos: usize, batch_size: usize, params: &BatchParams) -> Result<usize> {
        for i in 0..batch_size {
            if let Some(bytes) = params.get_str_data_at(pos, i) {
                let s = &mut self[i];
                let new_len = bytes.len().min(s.capacity());
                unsafe {
                    let vec = s.as_mut_vec();
                    vec.resize(new_len, 0);
                    std::ptr::copy_nonoverlapping(bytes.as_ptr(), vec.as_mut_ptr(), new_len);
                }
            } else {
                // NULL or empty result — clear the string
                unsafe { self[i].as_mut_vec().clear(); }
            }
        }
        Ok(pos + 1)
    }
}

// ─── Named parameters ─────────────────────────────────────────────────────────

impl<T: ToBatchSql> ToBatchSql for (&str, T) {
    fn bind_batch_to(&mut self, _pos: usize, batch_size: usize, params: &mut BatchParams, stmt: &OCIStmt, err: &OCIError) -> Result<usize> {
        let idx = params.index_of(self.0)?;
        self.1.bind_batch_to(idx, batch_size, params, stmt, err)
    }

    fn update_batch_from_bind(&mut self, _pos: usize, batch_size: usize, params: &BatchParams) -> Result<usize> {
        let idx = params.index_of(self.0)?;
        self.1.update_batch_from_bind(idx, batch_size, params)
    }
}

impl<T1, T2> ToBatchSql for ((&str, T1), (&str, T2)) where T1: ToBatchSql, T2: ToBatchSql {
    fn bind_batch_to(&mut self, _pos: usize, batch_size: usize, params: &mut BatchParams, stmt: &OCIStmt, err: &OCIError) -> Result<usize> {
        let idx = params.index_of(self.0.0)?;
        self.0.1.bind_batch_to(idx, batch_size, params, stmt, err)?;
        let idx = params.index_of(self.1.0)?;
        self.1.1.bind_batch_to(idx, batch_size, params, stmt, err)
    }

    fn update_batch_from_bind(&mut self, _pos: usize, batch_size: usize, params: &BatchParams) -> Result<usize> {
        let idx = params.index_of(self.0.0)?;
        self.0.1.update_batch_from_bind(idx, batch_size, params)?;
        let idx = params.index_of(self.1.0)?;
        self.1.1.update_batch_from_bind(idx, batch_size, params)
    }
}

// ─── Unit — no-op ─────────────────────────────────────────────────────────────

impl ToBatchSql for () {
    fn bind_batch_to(&mut self, pos: usize, _batch_size: usize, _params: &mut BatchParams, _stmt: &OCIStmt, _err: &OCIError) -> Result<usize> {
        Ok(pos + 1)
    }
}

// ─── Tuple macro ─────────────────────────────────────────────────────────────

macro_rules! impl_batch_tuple {
    ($($item:ident)+) => {
        impl<$($item),+> ToBatchSql for ($($item),+) where $($item: ToBatchSql),+ {
            #[allow(non_snake_case)]
            fn bind_batch_to(&mut self, mut pos: usize, batch_size: usize, params: &mut BatchParams, stmt: &OCIStmt, err: &OCIError) -> Result<usize> {
                let ($(ref mut $item),+) = *self;
                $(
                    pos = $item.bind_batch_to(pos, batch_size, params, stmt, err)?;
                )+
                Ok(pos)
            }
            #[allow(non_snake_case)]
            fn update_batch_from_bind(&mut self, mut pos: usize, batch_size: usize, params: &BatchParams) -> Result<usize> {
                let ($(ref mut $item),+) = *self;
                $(
                    pos = $item.update_batch_from_bind(pos, batch_size, params)?;
                )+
                Ok(pos)
            }
        }
    };
}

impl_batch_tuple! { A B C }
impl_batch_tuple! { A B C D }
impl_batch_tuple! { A B C D E }
impl_batch_tuple! { A B C D E F }
impl_batch_tuple! { A B C D E F G }
impl_batch_tuple! { A B C D E F G H }
impl_batch_tuple! { A B C D E F G H I }
impl_batch_tuple! { A B C D E F G H I J }
impl_batch_tuple! { A B C D E F G H I J K }
impl_batch_tuple! { A B C D E F G H I J K L }
impl_batch_tuple! { A B C D E F G H I J K L M }
impl_batch_tuple! { A B C D E F G H I J K L M N }
impl_batch_tuple! { A B C D E F G H I J K L M N O }
impl_batch_tuple! { A B C D E F G H I J K L M N O P }

// ─── Error for types that would be invalid in batch context ──────────────────

/// Returns a clear error when a single value is mistakenly passed to `execute_batch`.
macro_rules! impl_invalid_single_batch {
    ($($t:ty),+) => {
        $(
            impl ToBatchSql for $t {
                fn bind_batch_to(&mut self, _pos: usize, _batch_size: usize, _params: &mut BatchParams, _stmt: &OCIStmt, _err: &OCIError) -> Result<usize> {
                    Err(Error::new(concat!(
                        "Single value of type ", stringify!($t),
                        " cannot be used with execute_batch. Use a slice instead."
                    )))
                }
            }
        )+
    };
}

impl_invalid_single_batch! { i8, i16, i32, i64, isize }
impl_invalid_single_batch! { u8, u16, u32, u64, usize }
impl_invalid_single_batch! { f32, f64 }
impl_invalid_single_batch! { &str, String }

//! Batch parameter binding for array DML execution

use crate::{Result, Error, oci::{self, *}};
use std::{ptr, collections::HashMap};
use libc::c_void;

/// Manages parameter bindings for batch (array) DML execution.
///
/// Unlike [`Params`] which manages a single value per parameter, `BatchParams`
/// manages contiguous arrays of values, null indicators, and data lengths — one
/// element per row in the batch.
pub struct BatchParams {
    /// Parameter placeholder name → placeholder index mapping
    idxs: HashMap<String, usize>,
    /// OCI bind handles (one per unique placeholder)
    binds: Vec<Ptr<OCIBind>>,
    /// Contiguous arrays of NULL indicators (batch_size per parameter)
    nulls: Vec<Vec<i16>>,
    /// Contiguous arrays of actual data lengths (batch_size per parameter)
    data_lens: Vec<Vec<u32>>,
    /// Tracks which placeholders have been bound in this call
    bind_order: Vec<u16>,
    /// Contiguous value buffers (element_size * batch_size per parameter)
    buffers: Vec<Vec<u8>>,
    /// Size of each element in the buffer (stride for OCIBindByPos2)
    element_sizes: Vec<usize>,
    /// Number of rows in this batch
    batch_size: usize,
}

impl BatchParams {
    /// Discovers the bind placeholders in the prepared statement.
    pub(super) fn new(stmt: &OCIStmt, err: &OCIError, batch_size: usize) -> Result<Option<Self>> {
        let num_binds: u32 = attr::get(OCI_ATTR_BIND_COUNT, OCI_HTYPE_STMT, stmt, err)?;
        if num_binds == 0 {
            Ok(None)
        } else {
            let num_binds = num_binds as usize;
            let mut idxs  = HashMap::with_capacity(num_binds);
            let mut binds = Vec::with_capacity(num_binds);

            let mut bind_names      = vec![     ptr::null_mut::<u8>(); num_binds];
            let mut bind_name_lens  = vec![                       0u8; num_binds];
            let mut ind_names       = vec![     ptr::null_mut::<u8>(); num_binds];
            let mut ind_name_lens   = vec![                       0u8; num_binds];
            let mut dups            = vec![                       0u8; num_binds];
            let mut oci_binds       = vec![ptr::null_mut::<OCIBind>(); num_binds];
            let mut found: i32      = 0;

            oci::stmt_get_bind_info(
                stmt, err,
                num_binds as u32, 1, &mut found,
                bind_names.as_mut_ptr(), bind_name_lens.as_mut_ptr(),
                ind_names.as_mut_ptr(),  ind_name_lens.as_mut_ptr(),
                dups.as_mut_ptr(),       oci_binds.as_mut_ptr()
            )?;

            for i in 0..found as usize {
                if dups[i] == 0 {
                    let name = unsafe { std::slice::from_raw_parts(bind_names[i], bind_name_lens[i] as usize) };
                    let name = unsafe { std::str::from_utf8_unchecked(name) };
                    idxs.insert(name.to_string(), i);
                }
                binds.push(Ptr::new(oci_binds[i]));
            }

            Ok(Some(Self {
                idxs,
                binds,
                nulls:        vec![Vec::new(); num_binds],
                data_lens:    vec![Vec::new(); num_binds],
                bind_order:   Vec::with_capacity(num_binds),
                buffers:      vec![Vec::new(); num_binds],
                element_sizes: vec![0; num_binds],
                batch_size,
            }))
        }
    }

    /// Returns the bind name without an optional leading colon.
    fn strip_colon(name: &str) -> &str {
        if name.starts_with(':') { &name[1..] } else { name }
    }

    /// Returns the placeholder index for the given name.
    pub(crate) fn index_of(&self, name: &str) -> Result<usize> {
        let name = Self::strip_colon(name);
        if let Some(&ix) = self.idxs.get(name) {
            Ok(ix)
        } else if let Some(&ix) = self.idxs.get(name.to_uppercase().as_str()) {
            Ok(ix)
        } else {
            Err(Error::msg(format!("Statement does not define parameter placeholder {}", name)))
        }
    }

    /// Validates that the provided slice length matches the batch size.
    pub(crate) fn check_batch_len(&self, len: usize, param_name: &str) -> Result<()> {
        if len != self.batch_size {
            Err(Error::msg(format!(
                "Parameter '{}' has {} elements but batch size is {}",
                param_name, len, self.batch_size
            )))
        } else {
            Ok(())
        }
    }

    /// Core method: bind a contiguous array of fixed-size values.
    ///
    /// `valuep` points to the first element of the array.  
    /// `element_size` is the byte size of each element (stride).  
    /// `sql_type` is the OCI SQL type code.
    pub(crate) fn bind_batch(
        &mut self,
        idx: usize,
        sql_type: u16,
        valuep: *mut c_void,
        element_size: usize,
        stmt: &OCIStmt,
        err: &OCIError,
    ) -> Result<()> {
        self.bind_order.push(idx as u16);
        self.element_sizes[idx] = element_size;
        oci::bind_by_pos_batch(
            stmt,
            self.binds[idx].as_mut_ptr(),
            err,
            (idx + 1) as u32,
            valuep,
            element_size as i64,
            sql_type,
            self.nulls[idx].as_mut_ptr(),
            self.data_lens[idx].as_mut_ptr(),
            OCI_DEFAULT,
        )?;
        oci::bind_array_of_struct(
            self.binds[idx].get_mut(),
            err,
            element_size as u32,
            2, // size of i16 for null indicator skip
            4, // size of u32 for data length skip
            0, // no rcodeskip
        )
    }

    /// Bind a contiguous array of fixed-size IN values, copying into an internal buffer.
    ///
    /// All elements must have the same fixed size (numeric types).
    pub(crate) fn bind_batch_in_fixed(
        &mut self,
        idx: usize,
        sql_type: u16,
        data: *const c_void,
        element_size: usize,
        stmt: &OCIStmt,
        err: &OCIError,
    ) -> Result<()> {
        let total = element_size * self.batch_size;
        let buf_ptr = {
            let buf = &mut self.buffers[idx];
            buf.resize(total, 0u8);
            unsafe {
                std::ptr::copy_nonoverlapping(data as *const u8, buf.as_mut_ptr(), total);
            }
            buf.as_mut_ptr()
        };
        // All elements are NOT NULL with fixed size
        self.nulls[idx] = vec![OCI_IND_NOTNULL; self.batch_size];
        self.data_lens[idx] = vec![element_size as u32; self.batch_size];
        self.bind_batch(idx, sql_type, buf_ptr as _, element_size, stmt, err)
    }

    /// Bind a contiguous string buffer for batch IN string values.
    ///
    /// Strings are packed into a padded buffer where each slot is `max_len` bytes.
    /// `strings[i]` is a `(ptr, len)` pair for the i-th string.
    pub(crate) fn bind_batch_str_in(
        &mut self,
        idx: usize,
        strings: &[(*const u8, usize)],
        stmt: &OCIStmt,
        err: &OCIError,
    ) -> Result<()> {
        let max_len = strings.iter().map(|&(_, l)| l).max().unwrap_or(1).max(1);
        let total = max_len * self.batch_size;
        let mut nulls = vec![OCI_IND_NOTNULL; self.batch_size];
        let mut lens  = vec![0u32; self.batch_size];
        let buf_ptr = {
            let buf = &mut self.buffers[idx];
            buf.resize(total, 0u8);
            // Pack each string into its slot (no padding needed — OCI uses data_lens)
            for (i, &(ptr, len)) in strings.iter().enumerate() {
                let slot_start = i * max_len;
                if len == 0 {
                    nulls[i] = OCI_IND_NULL;
                } else {
                    unsafe {
                        std::ptr::copy_nonoverlapping(ptr, buf.as_mut_ptr().add(slot_start), len);
                    }
                    lens[i] = len as u32;
                }
            }
            buf.as_mut_ptr()
        };
        self.nulls[idx] = nulls;
        self.data_lens[idx] = lens;
        self.bind_batch(idx, SQLT_CHR, buf_ptr as _, max_len, stmt, err)
    }

    /// Bind a contiguous string buffer for batch OUT/INOUT string values.
    ///
    /// Each string slot gets `capacity` bytes. OCI writes results directly
    /// into the buffer, and data_lens entries are updated with actual lengths.
    pub(crate) fn bind_batch_str_out(
        &mut self,
        idx: usize,
        capacity: usize,
        stmt: &OCIStmt,
        err: &OCIError,
    ) -> Result<()> {
        let total = capacity * self.batch_size;
        let buf_ptr = {
            let buf = &mut self.buffers[idx];
            buf.resize(total, 0u8);
            buf.as_mut_ptr()
        };
        // Initialize null indicators and lengths for OUT params
        self.nulls[idx] = vec![OCI_IND_NULL; self.batch_size];
        self.data_lens[idx] = vec![capacity as u32; self.batch_size];
        self.bind_batch(idx, SQLT_CHR, buf_ptr as _, capacity, stmt, err)
    }

    pub(crate) fn bind_batch_out_fixed(
        &mut self,
        idx: usize,
        sql_type: u16,
        valuep: *mut c_void,
        element_size: usize,
        stmt: &OCIStmt,
        err: &OCIError,
    ) -> Result<()> {
        self.nulls[idx] = vec![OCI_IND_NULL; self.batch_size];
        self.data_lens[idx] = vec![element_size as u32; self.batch_size];
        self.bind_batch(idx, sql_type, valuep, element_size, stmt, err)
    }

    /// Bind a nullable fixed-size IN array.
    ///
    /// `values_and_nulls` is a slice of `(ptr_to_val_or_null, is_null, element_size)`.
    pub(crate) fn bind_batch_nullable_fixed(
        &mut self,
        idx: usize,
        sql_type: u16,
        element_size: usize,
        values: &[Option<*const c_void>],
        stmt: &OCIStmt,
        err: &OCIError,
    ) -> Result<()> {
        let total = element_size * self.batch_size;
        let mut nulls = vec![OCI_IND_NULL; self.batch_size];
        let mut lens  = vec![0u32; self.batch_size];
        let buf_ptr = {
            let buf = &mut self.buffers[idx];
            buf.resize(total, 0u8);
            for (i, opt) in values.iter().enumerate() {
                if let Some(ptr) = opt {
                    let slot = unsafe { buf.as_mut_ptr().add(i * element_size) };
                    unsafe { std::ptr::copy_nonoverlapping(*ptr as *const u8, slot, element_size); }
                    nulls[i] = OCI_IND_NOTNULL;
                    lens[i]  = element_size as u32;
                }
            }
            buf.as_mut_ptr()
        };
        self.nulls[idx] = nulls;
        self.data_lens[idx] = lens;
        self.bind_batch(idx, sql_type, buf_ptr as _, element_size, stmt, err)
    }

    pub(crate) fn bind_batch_nullable_out_fixed(
        &mut self,
        idx: usize,
        sql_type: u16,
        element_size: usize,
        init_data: *const c_void,
        stmt: &OCIStmt,
        err: &OCIError,
    ) -> Result<()> {
        let total = element_size * self.batch_size;
        let buf_ptr = {
            let buf = &mut self.buffers[idx];
            buf.resize(total, 0u8);
            if !init_data.is_null() {
                unsafe {
                    std::ptr::copy_nonoverlapping(init_data as *const u8, buf.as_mut_ptr(), total);
                }
            }
            buf.as_mut_ptr()
        };
        self.nulls[idx] = vec![OCI_IND_NULL; self.batch_size];
        self.data_lens[idx] = vec![element_size as u32; self.batch_size];
        self.bind_batch(idx, sql_type, buf_ptr as _, element_size, stmt, err)
    }

    /// Returns the null indicator for the given row in the given parameter slot.
    pub(crate) fn is_null_at(&self, idx: usize, row: usize) -> bool {
        self.nulls.get(idx)
            .and_then(|v| v.get(row))
            .map(|&ind| ind == OCI_IND_NULL)
            .unwrap_or(true)
    }

    /// Returns the data length for the given row and parameter slot.
    pub(crate) fn get_data_len_at(&self, idx: usize, row: usize) -> usize {
        self.data_lens.get(idx)
            .and_then(|v| v.get(row))
            .map(|&l| l as usize)
            .unwrap_or(0)
    }

    /// Returns a slice of the string buffer for the given parameter and row.
    pub(crate) fn get_str_data_at(&self, idx: usize, row: usize) -> Option<&[u8]> {
        let elem_size = self.element_sizes.get(idx).copied().unwrap_or(0);
        if elem_size == 0 { return None; }
        let len = self.get_data_len_at(idx, row);
        self.buffers.get(idx).and_then(|buf| {
            let start = row * elem_size;
            buf.get(start..start + len)
        })
    }

    /// Returns a pointer to the fixed-size element at the given row in the buffer.
    pub(crate) fn get_fixed_data_at<T: Copy>(&self, idx: usize, row: usize) -> Option<T> {
        let elem_size = self.element_sizes.get(idx).copied().unwrap_or(0);
        if elem_size == 0 || elem_size != std::mem::size_of::<T>() { return None; }
        self.buffers.get(idx).and_then(|buf| {
            let start = row * elem_size;
            buf.get(start..start + elem_size).map(|s| {
                let mut val = std::mem::MaybeUninit::<T>::uninit();
                unsafe {
                    std::ptr::copy_nonoverlapping(s.as_ptr(), val.as_mut_ptr() as *mut u8, elem_size);
                    val.assume_init()
                }
            })
        })
    }

    /// Returns the batch size.
    #[allow(dead_code)]
    pub(crate) fn batch_size(&self) -> usize {
        self.batch_size
    }
}

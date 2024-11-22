/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

use cpython::*;

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Default, Hash, Ord)]
pub struct Bytes(pub Box<[u8]>);

impl ToPyObject for Bytes {
    type ObjectType = PyBytes;

    #[inline]
    fn to_py_object(&self, py: Python) -> PyBytes {
        PyBytes::new(py, &self.0)
    }
}

impl<'source> FromPyObject<'source> for Bytes {
    fn extract(py: Python, obj: &'source PyObject) -> PyResult<Self> {
        let data = obj.cast_as::<PyBytes>(py)?.data(py);
        Ok(Bytes(data.to_vec().into_boxed_slice()))
    }
}

impl From<Box<[u8]>> for Bytes {
    fn from(v: Box<[u8]>) -> Bytes {
        Bytes(v)
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(v: Vec<u8>) -> Bytes {
        Bytes(v.into_boxed_slice())
    }
}

impl From<String> for Bytes {
    fn from(s: String) -> Bytes {
        Bytes(s.into_bytes().into_boxed_slice())
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

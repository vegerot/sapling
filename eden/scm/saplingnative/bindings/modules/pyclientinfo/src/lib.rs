/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#![allow(non_camel_case_types)]

use std::cell::RefCell;

use ::clientinfo as client_info;
use cpython::*;
use cpython_ext::convert::Serde;
use cpython_ext::ResultPyErrExt;

pub fn init_module(py: Python, package: &str) -> PyResult<PyModule> {
    let name = [package, "pyclientinfo"].join(".");
    let m = PyModule::new(py, &name)?;

    m.add_class::<clientinfo>(py)?;
    m.add_class::<ClientRequestInfo>(py)?;
    m.add(
        py,
        "get_client_request_info",
        py_fn!(py, get_client_request_info()),
    )?;
    m.add(
        py,
        "get_client_correlator",
        py_fn!(py, get_client_correlator()),
    )?;
    m.add(
        py,
        "get_client_entry_point",
        py_fn!(py, get_client_entry_point()),
    )?;
    Ok(m)
}

/// Convert `io::Result<Vec<u8>>` to a `PyResult<PyBytes>`.
fn convert<T>(py: Python, result: Result<Vec<u8>, T>) -> PyResult<PyBytes>
where
    T: std::fmt::Display,
{
    result
        .map_err(|e| PyErr::new::<exc::RuntimeError, _>(py, format!("{}", e)))
        .map(|buf| PyBytes::new(py, &buf))
}

py_class!(pub class clientinfo |py| {
    data clientinfo: RefCell<client_info::ClientInfo>;

    def __new__(_cls) -> PyResult<clientinfo> {
        let clientinfo = client_info::ClientInfo::new().map_pyerr(py)?;

        clientinfo::create_instance(py, RefCell::new(clientinfo))
    }

    def to_json(&self) -> PyResult<PyBytes> {
        convert(py, self.clientinfo(py).borrow().to_json().map(|s| s.into_bytes()))
    }
});

py_class!(pub class ClientRequestInfo |py| {
    data inner: RefCell<client_info::ClientRequestInfo>;

    def __new__(_cls, entry_point: &str) -> PyResult<ClientRequestInfo> {
        let entry_point = entry_point.try_into().map_pyerr(py)?;
        let client_request_info = client_info::ClientRequestInfo::new(entry_point);
        ClientRequestInfo::create_instance(py, RefCell::new(client_request_info))
    }
});

pub fn get_client_request_info(_py: Python) -> PyResult<Serde<client_info::ClientRequestInfo>> {
    Ok(Serde(client_info::get_client_request_info()))
}

pub fn get_client_correlator(py: Python) -> PyResult<PyBytes> {
    Ok(PyBytes::new(
        py,
        &client_info::get_client_request_info()
            .correlator
            .into_bytes(),
    ))
}

pub fn get_client_entry_point(py: Python) -> PyResult<PyBytes> {
    Ok(PyBytes::new(
        py,
        &client_info::get_client_request_info()
            .entry_point
            .to_string()
            .into_bytes(),
    ))
}

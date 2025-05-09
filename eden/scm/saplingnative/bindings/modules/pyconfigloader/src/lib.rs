/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

#![allow(non_camel_case_types)]

use std::cell::RefCell;
use std::sync::Arc;

use configloader::Config;
use configloader::config::ConfigSet;
use configloader::config::Options;
use configloader::convert::parse_list;
use configloader::hg::ConfigSetHgExt;
use configloader::hg::OptionsHgExt;
use configloader::hg::RepoInfo;
use cpython::*;
use cpython_ext::PyNone;
use cpython_ext::PyPath;
use cpython_ext::PyPathBuf;
use cpython_ext::error::Result;
use cpython_ext::error::ResultPyErrExt;
use repo_minimal_info::RepoMinimalInfo;

mod impl_into;

pub fn init_module(py: Python, package: &str) -> PyResult<PyModule> {
    let name = [package, "configloader"].join(".");
    let m = PyModule::new(py, &name)?;

    m.add_class::<config>(py)?;

    m.add(py, "parselist", py_fn!(py, parselist(value: String)))?;

    impl_into::register(py);

    Ok(m)
}

py_class!(pub class config |py| {
    data cfg: RefCell<ConfigSet>;

    def __new__(_cls) -> PyResult<config> {
        config::create_instance(py, RefCell::new(ConfigSet::new().named("pyconfig")))
    }

    def clone(&self) -> PyResult<config> {
        let cfg = self.cfg(py).borrow();
        config::create_instance(py, RefCell::new(cfg.clone()))
    }

    def readpath(
        &self,
        path: &PyPath,
        source: String,
        sections: Option<Vec<String>>,
        remap: Option<Vec<(String, String)>>,
    ) -> PyResult<Vec<String>> {
        let mut cfg = self.cfg(py).borrow_mut();

        let mut opts = Options::new().source(source).process_hgplain();
        if let Some(sections) = sections {
            opts = opts.filter_sections(sections);
        }
        if let Some(remap) = remap {
            let map = remap.into_iter().collect();
            opts = opts.remap_sections(map);
        }

        let errors = cfg.load_path(path, &opts);
        Ok(errors_to_str_vec(errors))
    }

    def parse(&self, content: String, source: String) -> PyResult<Vec<String>> {
        let mut cfg = self.cfg(py).borrow_mut();
        let opts = source.into();
        let errors = cfg.parse(content, &opts);
        Ok(errors_to_str_vec(errors))
    }

    def get(&self, section: &str, name: &str) -> PyResult<Option<PyString>> {
        let cfg = self.cfg(py).borrow();

        Ok(cfg.get(section, name).map(|v| PyString::new(py, &v)))
    }

    def sources(
        &self, section: &str, name: &str
    ) -> PyResult<Vec<(Option<PyString>, Option<(PyPathBuf, usize, usize, usize)>, PyString)>> {
        // Return [(value, file_source, source)]
        // file_source is a tuple of (file_path, byte_start, byte_end, line)
        let cfg = self.cfg(py).borrow();
        let sources = cfg.get_sources(section, name);
        let mut result = Vec::with_capacity(sources.len());
        for source in sources.as_ref().iter() {
            let value = source.value().as_ref().map(|v| PyString::new(py, v));
            let file = source.location().map(|(path, range)| {
                let line = source.line_number().unwrap_or_default();

                let pypath = if path.as_os_str().is_empty() {
                    PyPathBuf::from(String::from("<builtin>"))
                } else {
                    let path = util::path::strip_unc_prefix(&path);
                    path.try_into().unwrap()
                };
                (pypath, range.start, range.end, line)
            });
            let source = PyString::new(py, source.source());
            result.push((value, file, source));
        }
        Ok(result)
    }

    def set(
        &self, section: String, name: String, value: Option<String>, source: String
    ) -> PyResult<PyNone> {
        let mut cfg = self.cfg(py).borrow_mut();
        let opts = source.into();
        cfg.set(section, name, value, &opts);
        Ok(PyNone)
    }

    def sections(&self) -> PyResult<Vec<PyString>> {
        let cfg = self.cfg(py).borrow();
        Ok(cfg.sections().iter().map(|s| PyString::new(py, s)).collect())
    }

    def names(&self, section: &str) -> PyResult<Vec<PyString>> {
        let cfg = self.cfg(py).borrow();
        Ok(cfg.keys(section).iter().map(|s| PyString::new(py, s)).collect())
    }

    def tostring(&self) -> PyResult<String> {
        let cfg = self.cfg(py).borrow();
        Ok(cfg.to_string())
    }

    @staticmethod
    def load(repopath: Option<PyPathBuf>) -> PyResult<Self> {
        let info = path_to_info(py, repopath)?;
        let info = match info {
            Some(ref info) => RepoInfo::Disk(info),
            None => RepoInfo::NoRepo,
        };
        let mut cfg = ConfigSet::new();
        cfg.load(info, Default::default()).map_pyerr(py)?;
        Self::create_instance(py, RefCell::new(cfg))
    }

    def reload(
        &self,
        repopath: Option<PyPathBuf>,
    ) -> PyResult<PyNone> {
        let info = path_to_info(py, repopath)?;
        let info = match info {
            Some(ref info) => RepoInfo::Disk(info),
            None => RepoInfo::NoRepo,
        };
        let mut cfg = self.cfg(py).borrow_mut();
        cfg.load(info, Default::default()).map_pyerr(py)?;
        Ok(PyNone)
    }

    def files(&self) -> PyResult<Vec<PyPathBuf>> {
        self.cfg(py).borrow().files().iter().map(|(p, _)| p.as_path().try_into()).collect::<Result<Vec<PyPathBuf>>>().map_pyerr(py)
    }
});

fn path_to_info(py: Python, path: Option<PyPathBuf>) -> PyResult<Option<RepoMinimalInfo>> {
    // Ideally the callsite can provide `info` directly.
    let info = match path {
        None => None,
        Some(p) => Some(RepoMinimalInfo::from_repo_root(p.to_path_buf()).map_pyerr(py)?),
    };
    Ok(info)
}

impl config {
    pub fn get_cfg(&self, py: Python) -> ConfigSet {
        self.cfg(py).clone().into_inner()
    }

    pub(crate) fn get_config_trait(&self, py: Python) -> Arc<dyn Config> {
        Arc::new(self.get_cfg(py))
    }

    pub(crate) fn get_thread_safe_config_trait(&self, py: Python) -> Arc<dyn Config + Send + Sync> {
        Arc::new(self.get_cfg(py))
    }

    pub fn from_dyn_config(py: Python, config: Arc<dyn Config>) -> PyResult<Self> {
        let mut cfg = ConfigSet::new();
        cfg.secondary(config);
        Self::create_instance(py, RefCell::new(cfg))
    }
}

fn parselist(py: Python, value: String) -> PyResult<Vec<PyString>> {
    Ok(parse_list(value)
        .iter()
        .map(|v| PyString::new(py, v))
        .collect())
}

fn errors_to_str_vec(errors: Vec<configloader::error::Error>) -> Vec<String> {
    errors.into_iter().map(|err| format!("{}", err)).collect()
}

/*
 * Copyright (c) Meta Platforms, Inc. and affiliates.
 *
 * This software may be used and distributed according to the terms of the
 * GNU General Public License version 2.
 */

use std::collections::HashMap;

use clidispatch::global_flags::HgGlobalOpts;
use cliparser::alias::expand_aliases;
use cliparser::parser::Flag;
use cliparser::parser::ParseError;
use cliparser::parser::ParseOptions;
use cliparser::parser::StructFlags;
use cliparser::parser::Value;
use configmodel::Config;
use cpython::*;
use pyconfigloader::config;

mod exceptions {
    use super::*;

    py_exception!(cliparser, AmbiguousCommand);
    py_exception!(cliparser, CircularReference);
    py_exception!(cliparser, MalformedAlias);
    py_exception!(cliparser, OptionNotRecognized);
    py_exception!(cliparser, OptionRequiresArgument);
    py_exception!(cliparser, OptionArgumentInvalid);
    py_exception!(cliparser, OptionAmbiguous);
}

pub fn init_module(py: Python, package: &str) -> PyResult<PyModule> {
    let name = [package, "cliparser"].join(".");
    let m = PyModule::new(py, &name)?;
    m.add(py, "earlyparse", py_fn!(py, early_parse(args: Vec<String>)))?;
    m.add(py, "parseargs", py_fn!(py, parse_args(args: Vec<String>)))?;
    m.add(
        py,
        "parse",
        py_fn!(py, parse(args: Vec<String>, keep_sep: bool)),
    )?;
    m.add(
        py,
        "expandargs",
        py_fn!(py, expand_args(config: config, args: Vec<String>,)),
    )?;
    m.add(
        py,
        "parsecommand",
        py_fn!(
            py,
            parse_command(args: Vec<String>, definitions: Vec<FlagDef>)
        ),
    )?;
    {
        use exceptions::*;
        m.add(py, "AmbiguousCommand", AmbiguousCommand::type_object(py))?;
        m.add(py, "CircularReference", CircularReference::type_object(py))?;
        m.add(py, "MalformedAlias", MalformedAlias::type_object(py))?;
        m.add(
            py,
            "OptionNotRecognized",
            OptionNotRecognized::type_object(py),
        )?;
        m.add(
            py,
            "OptionRequiresArgument",
            OptionRequiresArgument::type_object(py),
        )?;
        m.add(
            py,
            "OptionArgumentInvalid",
            OptionArgumentInvalid::type_object(py),
        )?;
        m.add(py, "OptionAmbiguous", OptionAmbiguous::type_object(py))?;
    }
    Ok(m)
}

struct FlagDef {
    short: Option<char>,
    long: String,
    default: Value,
    flag_type: String,
}

impl<'s> FromPyObject<'s> for FlagDef {
    fn extract(py: Python, obj: &'s PyObject) -> PyResult<Self> {
        let tuple: PyTuple = obj.extract(py)?;
        if tuple.len(py) < 3 {
            let msg = "flag defintion requires at least 3 items".to_string();
            return Err(PyErr::new::<exc::ValueError, _>(py, msg));
        }
        let short: String = tuple.get_item(py, 0).extract(py)?;
        let long: String = tuple.get_item(py, 1).extract(py)?;
        let default: Value = tuple.get_item(py, 2).extract(py)?;
        let flag_type: String = if tuple.len(py) >= 4 {
            tuple.get_item(py, 3).extract(py)?
        } else {
            "".into()
        };
        Ok(FlagDef {
            short: short.chars().next(),
            long,
            default,
            flag_type,
        })
    }
}

impl From<FlagDef> for Flag {
    fn from(val: FlagDef) -> Self {
        let description = "";
        (val.short, val.long, description, val.default, val.flag_type).into()
    }
}

fn parse_command(
    py: Python,
    args: Vec<String>,
    definitions: Vec<FlagDef>,
) -> PyResult<(Vec<String>, HashMap<String, Value>, Vec<String>)> {
    let flags: Vec<Flag> = definitions.into_iter().map(Into::into).collect();

    let result = ParseOptions::new()
        .flag_alias("repo", "repository")
        .flags(flags)
        .error_on_unknown_opts(true)
        .parse_args(&args)
        .map_err(|e| map_to_python_err(py, e))?;

    let options: HashMap<String, Value> = result
        .opts()
        .iter()
        .map(|(k, v)| (k.replace('-', "_"), v.clone()))
        .collect();

    let specified = result.specified_opts().to_vec();

    Ok((result.args, options, specified))
}

fn expand_args(
    py: Python,
    config: config,
    args: Vec<String>,
) -> PyResult<(Vec<String>, Vec<String>)> {
    let cfg = &config.get_cfg(py);

    let lookup = move |name: &str| {
        if name.contains(':') {
            return None;
        }
        match (cfg.get("alias", name), cfg.get("defaults", name)) {
            (None, None) => None,
            (Some(v), None) => Some(v.to_string()),
            (None, Some(v)) => Some(format!("{} {}", name, v.as_ref())),
            (Some(a), Some(d)) => {
                // XXX: This makes defaults override alias if there are conflicted
                // flags. The desired behavior is to make alias override defaults.
                // However, [defaults] is deprecated and is likely only used
                // by tests. So this might be fine.
                Some(format!("{} {}", a.as_ref(), d.as_ref()))
            }
        }
    };

    let (expanded_args, replaced_aliases) =
        expand_aliases(lookup, &args).map_err(|e| map_to_python_err(py, e))?;

    Ok((expanded_args, replaced_aliases))
}

fn early_parse(py: Python, args: Vec<String>) -> PyResult<HashMap<String, PyObject>> {
    let result = ParseOptions::new()
        .ignore_prefix(true)
        .early_parse(true)
        .flag_alias("repo", "repository")
        .flags(HgGlobalOpts::flags())
        .parse_args(&args)
        .map_err(|e| map_to_python_err(py, e))?;
    let rust_opts = result.opts().clone();
    let mut opts = HashMap::new();

    for (key, value) in rust_opts {
        let val: PyObject = value.into_py_object(py).into_object();
        opts.insert(key, val);
    }
    Ok(opts)
}

fn parse_args(py: Python, args: Vec<String>) -> PyResult<Vec<String>> {
    let result = ParseOptions::new()
        .flag_alias("repo", "repository")
        .flags(HgGlobalOpts::flags())
        .parse_args(&args)
        .map_err(|e| map_to_python_err(py, e))?;
    let arguments = result.args().clone();

    Ok(arguments)
}

fn parse(
    py: Python,
    args: Vec<String>,
    keep_sep: bool,
) -> PyResult<(Vec<String>, HashMap<String, PyObject>, usize)> {
    let result = ParseOptions::new()
        .flag_alias("repo", "repository")
        .flags(HgGlobalOpts::flags())
        .keep_sep(keep_sep)
        .parse_args(&args)
        .map_err(|e| map_to_python_err(py, e))?;

    let arguments = result.args().to_vec();
    let opts = result
        .opts()
        .iter()
        .map(|(k, v)| (k.clone(), v.to_py_object(py).into_object()))
        .collect();

    Ok((arguments, opts, result.first_arg_index()))
}

fn map_to_python_err(py: Python, err: ParseError) -> PyErr {
    let msg = format!("{}", err);
    match err {
        ParseError::OptionNotRecognized { option_name } => {
            PyErr::new::<exceptions::OptionNotRecognized, _>(py, (msg, option_name))
        }
        ParseError::OptionRequiresArgument { option_name } => {
            PyErr::new::<exceptions::OptionRequiresArgument, _>(py, (msg, option_name))
        }
        ParseError::OptionArgumentInvalid {
            option_name,
            given,
            expected,
        } => PyErr::new::<exceptions::OptionArgumentInvalid, _>(
            py,
            (msg, option_name, given, expected),
        ),
        ParseError::OptionAmbiguous {
            option_name,
            possibilities,
        } => PyErr::new::<exceptions::OptionAmbiguous, _>(py, (msg, option_name, possibilities)),
        ParseError::AmbiguousCommand {
            command_name,
            possibilities,
        } => PyErr::new::<exceptions::AmbiguousCommand, _>(py, (msg, command_name, possibilities)),
        ParseError::CircularReference { command_name } => {
            PyErr::new::<exceptions::CircularReference, _>(py, (msg, command_name))
        }
        ParseError::MalformedAlias { name, value } => {
            PyErr::new::<exceptions::MalformedAlias, _>(py, (msg, name, value))
        }
    }
}

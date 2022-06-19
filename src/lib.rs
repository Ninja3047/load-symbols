#![feature(is_some_with)]
#![feature(let_chains)]

use binaryninja::architecture::{Architecture, CoreArchitecture};
use binaryninja::binaryview::BinaryView;
use binaryninja::binaryview::BinaryViewExt;
use binaryninja::callingconvention::CallingConvention;
use binaryninja::debuginfo::{
    CustomDebugInfoParser, DebugFunctionInfo, DebugInfo, DebugInfoParser,
};
use binaryninja::logger;
use binaryninja::platform::Platform;
use binaryninja::rc::Ref;
use binaryninja::string::BnStrCompatible;
use binaryninja::types::Type;

use cpp_demangle::DemangleOptions;
use object::{Object, ObjectSymbol, SymbolKind};

use derivative::Derivative;

use std::error::Error;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::prelude::OsStrExt;
use std::path::{Path, PathBuf};

use log::{error, info, warn, LevelFilter};

struct SymbolInfoParser;

#[derive(Derivative)]
#[derivative(Default(bound = ""))]
pub struct DebugFunctionInfoBuilder<A: Architecture, S1: BnStrCompatible, S2: BnStrCompatible> {
    short_name: Option<S1>,
    full_name: Option<S1>,
    raw_name: Option<S1>,
    return_type: Option<Ref<Type>>,
    address: Option<u64>,
    parameters: Option<Vec<(S2, Ref<Type>)>>,
    variable_parameters: Option<bool>,
    calling_convention: Option<Ref<CallingConvention<A>>>,
    platform: Option<Ref<Platform>>,
}

impl<A: Architecture, S1: BnStrCompatible, S2: BnStrCompatible>
    DebugFunctionInfoBuilder<A, S1, S2>
{
    pub fn new() -> Self {
        DebugFunctionInfoBuilder::default()
    }

    pub fn short_name(mut self, short_name: S1) -> Self {
        self.short_name = Some(short_name);
        self
    }

    pub fn full_name(mut self, full_name: S1) -> Self {
        self.full_name = Some(full_name);
        self
    }

    pub fn raw_name(mut self, raw_name: S1) -> Self {
        self.raw_name = Some(raw_name);
        self
    }

    pub fn address(mut self, address: u64) -> Self {
        self.address = Some(address);
        self
    }

    pub fn build(self) -> DebugFunctionInfo<A, S1, S2> {
        DebugFunctionInfo::new(
            self.short_name,
            self.full_name,
            self.raw_name,
            self.return_type,
            self.address,
            self.parameters,
            self.variable_parameters,
            self.calling_convention,
            self.platform,
        )
    }
}

fn demangle(s: &str) -> Result<String, Box<dyn Error>> {
    let sym = cpp_demangle::Symbol::new(s)?;
    let options = DemangleOptions::new().no_params().no_return_type();
    let s = sym.demangle(&options)?;
    Ok(s)
}

fn add_function(debug_info: &mut DebugInfo, symbol: object::Symbol) -> Result<(), Box<dyn Error>> {
    let name = symbol.name()?;
    let demangled = match demangle(name) {
        Ok(d) => d,
        _ => name.to_string(),
    };

    info!("Function added: {}: {:x?}", demangled, symbol.address());

    let new_func: DebugFunctionInfo<CoreArchitecture, &str, &str> = DebugFunctionInfoBuilder::new()
        .raw_name(name)
        .full_name(&demangled)
        .address(symbol.address())
        .build();
    debug_info.add_function(new_func);
    Ok(())
}

fn add_data(debug_info: &mut DebugInfo, symbol: object::Symbol) -> Result<(), Box<dyn Error>> {
    let new_type = Type::void();

    let name = symbol.name()?;
    let demangled = match demangle(name) {
        Ok(d) => d,
        _ => name.to_string(),
    };
    info!("Data added: {}: {:x?}", demangled, symbol.address());

    debug_info.add_data_variable(symbol.address(), &new_type, Some(demangled));
    Ok(())
}

fn get_symbols(debug_info: &mut DebugInfo, path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let file = fs::File::open(&path)?;
    let file = unsafe { memmap2::Mmap::map(&file) }?;
    let file = object::File::parse(&*file)?;
    file.symbols()
        .into_iter()
        .filter(|symbol| symbol.is_definition())
        .try_for_each(|symbol| match symbol.kind() {
            SymbolKind::Text => add_function(debug_info, symbol),
            SymbolKind::Data => add_data(debug_info, symbol),
            _ => Ok(()),
        })?;

    Ok(())
}

fn get_debug_path(view: &BinaryView) -> Option<PathBuf> {
    if let Ok(path) = fs::canonicalize(PathBuf::from(view.metadata().filename().to_string())) &&
       let Ok(path) = path.strip_prefix("/") {
        let f = Path::new("/usr/lib/debug")
            .join(path);
        let ext = match f.extension() {
            Some(ext) => {
                [ext.as_bytes(), b".debug"].concat()
            },
            None => b"debug".to_vec()
        };
        Some(f.with_extension(OsStr::from_bytes(ext.as_slice())))
    } else {
        None
    }
}

impl CustomDebugInfoParser for SymbolInfoParser {
    fn is_valid(&self, view: &BinaryView) -> bool {
        warn!("Checking for {:?}", get_debug_path(view));
        get_debug_path(view).is_some_and(|f| f.exists())
    }

    fn parse_info(&self, debug_info: &mut DebugInfo, view: &BinaryView) {
        if let Some(debug_path) = get_debug_path(view) {
            info!("Loading symbols from {}", debug_path.to_string_lossy());
            if let Err(err) = get_symbols(debug_info, &debug_path) {
                error!("Loading symbols failed {:?}", err);
            }
        } else {
            error!("Unable to load debug path");
        }
    }

    fn is_external(&self) -> bool {
        false
    }
}

#[no_mangle]
pub extern "C" fn CorePluginInit() -> bool {
    logger::init(LevelFilter::Info).unwrap();

    DebugInfoParser::register("Symbol info parser", SymbolInfoParser {});
    true
}

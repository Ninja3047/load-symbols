#![feature(is_some_and)]
#![feature(let_chains)]

use binaryninja::binaryview::BinaryView;
use binaryninja::binaryview::BinaryViewExt;
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
pub struct DebugFunctionInfoBuilder<S: BnStrCompatible> {
    short_name: Option<S>,
    full_name: Option<S>,
    raw_name: Option<S>,
    return_type: Option<Ref<Type>>,
    address: Option<u64>,
    platform: Option<Ref<Platform>>,
}

impl<S: BnStrCompatible> DebugFunctionInfoBuilder<S> {
    #[must_use]
    pub fn new() -> Self {
        DebugFunctionInfoBuilder::default()
    }

    pub fn short_name(mut self, short_name: S) -> Self {
        self.short_name = Some(short_name);
        self
    }

    pub fn full_name(mut self, full_name: S) -> Self {
        self.full_name = Some(full_name);
        self
    }

    pub fn raw_name(mut self, raw_name: S) -> Self {
        self.raw_name = Some(raw_name);
        self
    }

    pub fn address(mut self, address: u64) -> Self {
        self.address = Some(address);
        self
    }

    pub fn build(self) -> DebugFunctionInfo<S> {
        DebugFunctionInfo::new(
            self.short_name,
            self.full_name,
            self.raw_name,
            self.return_type,
            self.address,
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

fn add_function(debug_info: &mut DebugInfo, symbol: &object::Symbol) -> Result<(), Box<dyn Error>> {
    let name = symbol.name()?;
    let demangled = match demangle(name) {
        Ok(d) => d,
        _ => name.to_string(),
    };

    info!("Function added: {}: {:x?}", demangled, symbol.address());

    let new_func: DebugFunctionInfo<&str> = DebugFunctionInfoBuilder::new()
        .raw_name(name)
        .full_name(&demangled)
        .address(symbol.address())
        .build();
    debug_info.add_function(new_func);
    Ok(())
}

fn add_data(debug_info: &mut DebugInfo, symbol: &object::Symbol) -> Result<(), Box<dyn Error>> {
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
    let file = fs::File::open(path)?;
    let file = unsafe { memmap2::Mmap::map(&file) }?;
    let file = object::File::parse(&*file)?;
    file.symbols()
        .filter(ObjectSymbol::is_definition)
        .try_for_each(|symbol| match symbol.kind() {
            SymbolKind::Text => add_function(debug_info, &symbol),
            SymbolKind::Data => add_data(debug_info, &symbol),
            _ => Ok(()),
        })?;

    Ok(())
}

fn get_debug_path(view: &BinaryView) -> Option<PathBuf> {
    if let Ok(path) = fs::canonicalize(PathBuf::from(view.file().filename().to_string())) &&
       let Ok(path) = path.strip_prefix("/") {
        let f = Path::new("/usr/lib/debug")
            .join(path);
        let ext = match f.extension() {
            Some(ext) => {
                [ext.as_bytes(), b".debug"].concat()
            },
            None => b"debug".to_vec()
        };
        let debug_path = f.with_extension(OsStr::from_bytes(ext.as_slice()));
        info!("Loading symbols from {}", debug_path.to_string_lossy());
        Some(debug_path)
    } else {
        None
    }
}

impl CustomDebugInfoParser for SymbolInfoParser {
    fn is_valid(&self, view: &BinaryView) -> bool {
        warn!("Checking for {:?}", get_debug_path(view));
        get_debug_path(view).is_some_and(|f| f.exists())
    }

    fn parse_info(
        &self,
        debug_info: &mut DebugInfo,
        view: &BinaryView,
        _progress: Box<dyn Fn(usize, usize) -> Result<(), ()>>,
    ) -> bool {
        if let Some(debug_path) = get_debug_path(view) {
            if let Err(err) = get_symbols(debug_info, &debug_path) {
                error!("Loading symbols failed {:?}", err);
                return false;
            }
        } else {
            error!("Unable to load debug path");
            return false;
        }
        true
    }
}

#[no_mangle]
pub extern "C" fn CorePluginInit() -> bool {
    logger::init(LevelFilter::Info).unwrap();

    DebugInfoParser::register("Symbol info parser", SymbolInfoParser {});
    true
}

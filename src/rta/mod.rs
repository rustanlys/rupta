pub mod body_visitor;
pub mod rta; 

use log::*;
use rustc_driver::Compilation;
use rustc_interface::{interface, Queries};
use rustc_middle::ty::TyCtxt;

use crate::mir::analysis_context::AnalysisContext;
use crate::util::mem_watcher::MemoryWatcher;
use crate::util::options::AnalysisOptions;

pub struct RTACallbacks {
    /// Options provided to the analysis.
    pub options: AnalysisOptions,
    /// The relative path of the file being compiled.
    file_name: String,
}

/// Constructor
impl RTACallbacks {
    pub fn new(options: AnalysisOptions) -> RTACallbacks {
        RTACallbacks {
            options,
            file_name: String::new(),
        }
    }

    fn run_rapid_type_analysis(&mut self, compiler: &interface::Compiler, tcx: TyCtxt<'_>) {
        let mut mem_watcher = MemoryWatcher::new();
        mem_watcher.start();

        if let Some(mut acx) = AnalysisContext::new(&compiler.sess, tcx, self.options.clone()) {
            let mut rta = self::rta::RapidTypeAnalysis::new(&mut acx);
            rta.analyze();

            self.dump_call_graph(&rta);
        } else {
            error!("AnalysisContext Initialization Failed");
        }

        mem_watcher.stop();
    }

    fn dump_call_graph(&mut self, rta: &self::rta::RapidTypeAnalysis) {
        if let Some(cg_output) = &self.options.call_graph_output {
            let cg_path = std::path::Path::new(cg_output);
            rta.dump_call_graph(cg_path);
        }
    }

}

impl rustc_driver::Callbacks for RTACallbacks {
    /// Called before creating the compiler instance
    fn config(&mut self, config: &mut interface::Config) {
        self.file_name = config.input.source_name().prefer_remapped_unconditionaly().to_string();
        debug!("Processing input file: {}", self.file_name);
    }

    /// Called after the compiler has completed all analysis passes and before it lowers MIR to LLVM IR.
    /// At this point the compiler is ready to tell us all it knows and we can proceed to do abstract
    /// interpretation of all of the functions that will end up in the compiler output.
    /// If this method returns false, the compilation will stop.
    fn after_analysis<'tcx>(
        &mut self,
        compiler: &interface::Compiler,
        queries: &'tcx Queries<'tcx>,
    ) -> Compilation {
        compiler.sess.dcx().abort_if_errors();
        queries
            .global_ctxt()
            .unwrap()
            .enter(|tcx| self.run_rapid_type_analysis(compiler, tcx));
        Compilation::Continue
    }
}
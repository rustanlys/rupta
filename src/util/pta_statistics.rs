// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use log::*;
use std::collections::{HashMap, HashSet};
use std::io::{BufWriter, Write};
use std::rc::Rc;

use crate::mir::path::Path;
use crate::pta::andersen::AndersenPTA;
use crate::pta::context_sensitive::ContextSensitivePTA;
use crate::pta::context_strategy::ContextStrategy;
use crate::pts_set::points_to::PointsToSet;

pub struct AndersenStat<'pta, 'tcx, 'compilation> {
    pta: &'pta AndersenPTA<'pta, 'tcx, 'compilation>,
}

impl<'pta, 'tcx, 'compilation> AndersenStat<'pta, 'tcx, 'compilation> {
    pub fn new(pta: &'pta AndersenPTA<'pta, 'tcx, 'compilation>) -> Self {
        AndersenStat { pta }
    }

    pub fn dump_stats(&self) {
        let mut stat_writer = BufWriter::new(
            Box::new(std::io::stdout()) as Box<dyn Write>
        );

        info!("Dumping pta statistics...");
        stat_writer
            .write_all("##########################################################\n".as_bytes())
            .expect("Unable to write data");
        crate::util::call_graph_stat::ci_call_graph_stat(self.pta.acx, &self.pta.call_graph, &mut stat_writer);
        stat_writer
            .write_all("----------------------------------------------------------\n".as_bytes())
            .expect("Unable to write data");
        self.dump_pts_stat(&mut stat_writer);
        stat_writer
            .write_all("##########################################################\n".as_bytes())
            .expect("Unable to write data");
    }

    pub fn dump_pts_stat<W: Write>(&self, stat_writer: &mut BufWriter<W>) {
        let pts_map = &self.pta.pt_data.propa_pts_map;
        let num_pointers = pts_map.len();
        let mut num_pts_relations = 0;
        for (_ptr, pts) in pts_map {
            num_pts_relations += pts.count();
        }
        let avg_pts = num_pts_relations as f64 / num_pointers as f64;

        stat_writer
            .write_all("Points-to Statistics: \n".as_bytes())
            .expect("Unable to write data");
        stat_writer
            .write_all(format!("#Pointers: {}\n", num_pointers).as_bytes())
            .expect("Unable to write data");
        stat_writer
            .write_all(format!("#Points-to relations: {}\n", num_pts_relations).as_bytes())
            .expect("Unable to write data");
        stat_writer
            .write_all(format!("#Avg points-to size: {}\n", avg_pts).as_bytes())
            .expect("Unable to write data");
    }
}

pub struct ContextSensitiveStat<'pta, 'tcx, 'compilation, S: ContextStrategy> {
    pta: &'pta ContextSensitivePTA<'pta, 'tcx, 'compilation, S>,
}

impl<'pta, 'tcx, 'compilation, S: ContextStrategy> ContextSensitiveStat<'pta, 'tcx, 'compilation, S> {
    pub fn new(pta: &'pta ContextSensitivePTA<'pta, 'tcx, 'compilation, S>) -> Self {
        ContextSensitiveStat { pta }
    }

    pub fn dump_stats(&mut self) {
        let mut stat_writer = BufWriter::new(
            Box::new(std::io::stdout()) as Box<dyn Write>
        );

        info!("Dumping pta statistics...");
        stat_writer
            .write_all("##########################################################\n".as_bytes())
            .expect("Unable to write data");
        crate::util::call_graph_stat::cs_call_graph_stat(self.pta.acx, &self.pta.call_graph, &mut stat_writer);
        stat_writer
            .write_all("----------------------------------------------------------\n".as_bytes())
            .expect("Unable to write data");
        self.dump_pts_stat(&mut stat_writer);
        stat_writer
            .write_all("##########################################################\n".as_bytes())
            .expect("Unable to write data");
    }

    pub fn dump_pts_stat<W: Write>(&self, stat_writer: &mut BufWriter<W>) {
        let cs_pts_map = &self.pta.pt_data.propa_pts_map;
        let mut ci_pts_map: HashMap<Rc<Path>, HashSet<Rc<Path>>> = HashMap::new();
        let num_cs_pointers = cs_pts_map.len();
        let mut num_cs_pts_relations = 0;
        for (ptr_id, pts) in cs_pts_map {
            num_cs_pts_relations += pts.count();

            let cs_ptr_path = self.pta.pag.node_path(*ptr_id);
            let ci_ptr_path = cs_ptr_path.path.clone();
            let ci_pts = ci_pts_map.entry(ci_ptr_path).or_default();
            for pointee in pts {
                let cs_pointee_path = self.pta.pag.node_path(pointee);
                let ci_pointee_path = cs_pointee_path.path.clone();
                ci_pts.insert(ci_pointee_path);
            }
        }
        let avg_cs_pts = num_cs_pts_relations as f64 / num_cs_pointers as f64;

        let num_ci_pointers = ci_pts_map.len();
        let mut num_ci_pts_relations = 0;
        for (_ptr, pts) in ci_pts_map {
            num_ci_pts_relations += pts.len();
        }
        let avg_ci_pts = num_ci_pts_relations as f64 / num_ci_pointers as f64;

        stat_writer
            .write_all("CS Points-to Statistics: \n".as_bytes())
            .expect("Unable to write data");
        stat_writer
            .write_all(format!("#Pointers: {}\n", num_cs_pointers).as_bytes())
            .expect("Unable to write data");
        stat_writer
            .write_all(format!("#Points-to relations: {}\n", num_cs_pts_relations).as_bytes())
            .expect("Unable to write data");
        stat_writer
            .write_all(format!("#Avg points-to size: {}\n", avg_cs_pts).as_bytes())
            .expect("Unable to write data");

        stat_writer
            .write_all("CI Points-to Statistics: \n".as_bytes())
            .expect("Unable to write data");
        stat_writer
            .write_all(format!("#Pointers: {}\n", num_ci_pointers).as_bytes())
            .expect("Unable to write data");
        stat_writer
            .write_all(format!("#Points-to relations: {}\n", num_ci_pts_relations).as_bytes())
            .expect("Unable to write data");
        stat_writer
            .write_all(format!("#Avg points-to size: {}\n", avg_ci_pts).as_bytes())
            .expect("Unable to write data");
    }
}

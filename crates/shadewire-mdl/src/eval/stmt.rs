//! Statements under a mask: branches on run-time conditions run both sides, loops unroll, and `return`, `break`
//! and `continue` leave the mask false where they ran.

use shadewire::{Error, Value};

use super::{Eval, Flow, Frame, LoopFlow, MAX_ITERATIONS};
use crate::ast::{BinOp, Expr, Stmt, StmtKind};
use crate::types::Ty;
use crate::value::{Num, Val};

fn known(b: bool) -> Num {
    Num::Known(Value::Boolean(b))
}

impl<'p, 'r, 'a> Eval<'p, 'r, 'a> {
    pub fn exec(&mut self, f: &mut Frame, flow: &mut Flow, s: &Stmt) -> Result<(), Error> {
        if flow.dead() {
            return Ok(());
        }
        self.step(f.module, s.line, &f.function)?;
        let origin = Self::origin(f, s.line);
        let (module, function) = (f.module, f.function.clone());
        let fail = |e: &Self, m: String| e.bad(e.at(module, s.line, &function), m);
        match &s.kind {
            StmtKind::Block(body) => {
                f.scopes.push(Default::default());
                let result = body.iter().try_for_each(|b| self.exec(f, flow, b));
                f.scopes.pop();
                result
            }
            StmtKind::Var { ty, declarators } => {
                for d in declarators {
                    let v = self.declarator_value(f, flow, ty, d)?;
                    f.declare(&d.name, v);
                }
                Ok(())
            }
            StmtKind::Decl(kind) => Err(fail(self, format!("a local declaration ({kind:?}) is not supported"))),
            StmtKind::Expr(e) => {
                if let Some(e) = e {
                    self.expr(f, flow, e)?;
                }
                Ok(())
            }
            StmtKind::If { cond, then, otherwise } => {
                let c = self.condition(f, flow, cond)?;
                if let Some(k) = c.known() {
                    if k.as_bool() {
                        return self.exec_scoped(f, flow, then);
                    }
                    return match otherwise {
                        Some(o) => self.exec_scoped(f, flow, o),
                        None => Ok(()),
                    };
                }
                let outer = flow.mask.clone();
                flow.mask = self.and(&outer, &c, &origin).map_err(|m| fail(self, m))?;
                self.exec_scoped(f, flow, then)?;
                let after_then = std::mem::replace(&mut flow.mask, known(false));
                let not_c = self.not(&c, &origin).map_err(|m| fail(self, m))?;
                flow.mask = self.and(&outer, &not_c, &origin).map_err(|m| fail(self, m))?;
                if let Some(o) = otherwise {
                    self.exec_scoped(f, flow, o)?;
                }
                let after_else = flow.mask.clone();
                flow.mask = self.or(&after_then, &after_else, &origin).map_err(|m| fail(self, m))?;
                Ok(())
            }
            StmtKind::While { cond, body } => self.run_loop(f, flow, Some(cond), None, body, false, s.line),
            StmtKind::DoWhile { body, cond } => self.run_loop(f, flow, Some(cond), None, body, true, s.line),
            StmtKind::For { init, cond, update, body } => {
                f.scopes.push(Default::default());
                let result = (|| {
                    if let Some(init) = init {
                        self.exec(f, flow, init)?;
                    }
                    self.run_loop(f, flow, cond.as_ref(), update.as_ref(), body, false, s.line)
                })();
                f.scopes.pop();
                result
            }
            StmtKind::Switch { selector, cases } => self.switch(f, flow, selector, cases, s.line),
            StmtKind::Break => {
                let mask = std::mem::replace(&mut flow.mask, known(false));
                let Some(l) = flow.loops.last_mut() else { return Err(fail(self, "break outside a loop".into())) };
                let broke = l.broke.clone();
                let broke = self.or(&broke, &mask, &origin).map_err(|m| fail(self, m))?;
                flow.loops.last_mut().expect("a loop").broke = broke;
                Ok(())
            }
            StmtKind::Continue => {
                let mask = std::mem::replace(&mut flow.mask, known(false));
                let Some(index) = flow.loops.iter().rposition(|l| !l.switch) else {
                    return Err(fail(self, "continue outside a loop".into()));
                };
                let continued = flow.loops[index].continued.clone();
                flow.loops[index].continued = self.or(&continued, &mask, &origin).map_err(|m| fail(self, m))?;
                // A switch between here and the loop is left too.
                Ok(())
            }
            StmtKind::Return(value) => {
                let v = match value {
                    Some(e) => self.expr(f, flow, e)?,
                    None => Val::bool(false),
                };
                let v = match &flow.ret_ty {
                    Some(t) => {
                        let t = self.complete_array(t, &v);
                        self.convert(v, &t, false, s.line, f.module)?
                    }
                    None => v,
                };
                let mask = std::mem::replace(&mut flow.mask, known(false));
                flow.ret = Some(match flow.ret.take() {
                    None => v,
                    Some(old) => self.select(&mask, v, old, &origin).map_err(|m| fail(self, m))?,
                });
                Ok(())
            }
        }
    }

    /// A statement in a scope of its own (an `if` branch or a loop body that is not a block).
    fn exec_scoped(&mut self, f: &mut Frame, flow: &mut Flow, s: &Stmt) -> Result<(), Error> {
        f.scopes.push(Default::default());
        let result = self.exec(f, flow, s);
        f.scopes.pop();
        result
    }

    /// A condition as a boolean.
    fn condition(&mut self, f: &mut Frame, flow: &mut Flow, cond: &Expr) -> Result<Num, Error> {
        let origin = Self::origin(f, cond.line);
        let c = self.expr(f, flow, cond)?;
        let c = self.convert_val(c, &Ty::BOOL, true, &origin).map_err(|m| self.error(f, cond.line, m))?;
        Ok(c.num().expect("a bool").clone())
    }

    /// A loop, unrolled: each iteration runs where the condition held so far, until it is known false
    /// everywhere.
    #[allow(clippy::too_many_arguments)]
    fn run_loop(
        &mut self,
        f: &mut Frame,
        flow: &mut Flow,
        cond: Option<&Expr>,
        update: Option<&Expr>,
        body: &Stmt,
        test_after: bool,
        line: u32,
    ) -> Result<(), Error> {
        let origin = Self::origin(f, line);
        flow.loops.push(LoopFlow { broke: known(false), continued: known(false), switch: false });
        let mut exit = known(false);
        let mut iterations = 0usize;
        let result = (|| {
            loop {
                if (!test_after || iterations > 0)
                    && let Some(c) = cond
                {
                    let c = self.condition(f, flow, c)?;
                    let not_c = self.not(&c, &origin).map_err(|m| self.error(f, line, m))?;
                    let leaving = self.and(&flow.mask, &not_c, &origin).map_err(|m| self.error(f, line, m))?;
                    exit = self.or(&exit, &leaving, &origin).map_err(|m| self.error(f, line, m))?;
                    flow.mask = self.and(&flow.mask, &c, &origin).map_err(|m| self.error(f, line, m))?;
                }
                if flow.dead() {
                    return Ok(());
                }
                iterations += 1;
                if iterations > MAX_ITERATIONS {
                    return Err(Error::Limit(format!(
                        "{}: a loop runs more than {MAX_ITERATIONS} times or until a value known only at run time",
                        self.at(f.module, line, &f.function)
                    )));
                }
                self.exec_scoped(f, flow, body)?;
                let continued =
                    std::mem::replace(&mut flow.loops.last_mut().expect("this loop").continued, known(false));
                flow.mask = self.or(&flow.mask, &continued, &origin).map_err(|m| self.error(f, line, m))?;
                if let Some(u) = update
                    && !flow.dead()
                {
                    self.expr(f, flow, u)?;
                }
            }
        })();
        let l = flow.loops.pop().expect("this loop");
        result?;
        let still = flow.mask.clone();
        let left = self.or(&exit, &l.broke, &origin).map_err(|m| self.error(f, line, m))?;
        flow.mask = self.or(&left, &still, &origin).map_err(|m| self.error(f, line, m))?;
        Ok(())
    }

    /// `switch`: C's semantics, cases falling through until a `break`; `default` runs where no case matched.
    fn switch(
        &mut self,
        f: &mut Frame,
        flow: &mut Flow,
        selector: &Expr,
        cases: &[(Option<Expr>, Vec<Stmt>)],
        line: u32,
    ) -> Result<(), Error> {
        let origin = Self::origin(f, line);
        let v = self.expr(f, flow, selector)?;
        let outer = flow.mask.clone();
        // Where each labelled case matches, and where none does.
        let mut hits = Vec::new();
        let mut any = known(false);
        for (label, _) in cases {
            match label {
                Some(label) => {
                    let l = self.expr(f, flow, label)?;
                    let hit = self.binary(BinOp::Eq, v.clone(), l, &origin).map_err(|m| self.error(f, line, m))?;
                    let hit = hit.num().expect("a bool").clone();
                    any = self.or(&any, &hit, &origin).map_err(|m| self.error(f, line, m))?;
                    hits.push(Some(hit));
                }
                None => hits.push(None),
            }
        }
        let none = self.not(&any, &origin).map_err(|m| self.error(f, line, m))?;
        flow.loops.push(LoopFlow { broke: known(false), continued: known(false), switch: true });
        let mut active = known(false);
        let result = (|| {
            for ((_, body), hit) in cases.iter().zip(&hits) {
                let enter = hit.clone().unwrap_or_else(|| none.clone());
                let enter = self.and(&outer, &enter, &origin).map_err(|m| self.error(f, line, m))?;
                active = self.or(&active, &enter, &origin).map_err(|m| self.error(f, line, m))?;
                flow.mask = active.clone();
                f.scopes.push(Default::default());
                let r = body.iter().try_for_each(|s| self.exec(f, flow, s));
                f.scopes.pop();
                r?;
                active = flow.mask.clone();
            }
            Ok(())
        })();
        let l = flow.loops.pop().expect("the switch");
        result?;
        let mut after = self.or(&active, &l.broke, &origin).map_err(|m| self.error(f, line, m))?;
        if !hits.iter().any(Option::is_none) {
            // Without a default, where no case matched the switch is passed by.
            let passed = self.and(&outer, &none, &origin).map_err(|m| self.error(f, line, m))?;
            after = self.or(&after, &passed, &origin).map_err(|m| self.error(f, line, m))?;
        }
        flow.mask = after;
        Ok(())
    }
}

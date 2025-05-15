use clippy_utils::diagnostics::span_lint_and_sugg;
use clippy_utils::source::snippet;
use clippy_utils::sym;
use rustc_errors::Applicability;
use rustc_hir::{Expr, ExprKind};
use rustc_lint::{LateContext, LateLintPass};
use rustc_middle::ty::{self, ExistentialPredicate, Ty, TyCtxt};
use rustc_session::declare_lint_pass;

declare_clippy_lint! {
    /// ### What it does
    ///
    /// Detects cases where a `&dyn Any` is constructed directly referencing a `Box<dyn Any>` or
    /// other value that dereferences to `dyn Any`.
    ///
    /// ### Why is this bad?
    ///
    /// The intention is usually to borrow the `dyn Any` available by dereferencing the value,
    /// rather than the value itself.
    ///
    /// ### Example
    /// ```no_run
    /// # use std::any::Any;
    /// let x: Box<dyn Any> = Box::new(());
    /// let _: &dyn Any = &x;
    /// ```
    /// Use instead:
    /// ```no_run
    /// # use std::any::Any;
    /// let x: Box<dyn Any> = Box::new(());
    /// let _: &dyn Any = &*x;
    /// ```
    #[clippy::version = "1.88.0"]
    pub COERCE_ANY_REF_TO_ANY,
    nursery,
    "coercing to `&dyn Any` when dereferencing could produce a `dyn Any` without coercion is usually not intended"
}
declare_lint_pass!(CoerceAnyRefToAny => [COERCE_ANY_REF_TO_ANY]);

impl<'tcx> LateLintPass<'tcx> for CoerceAnyRefToAny {
    fn check_expr(&mut self, cx: &LateContext<'tcx>, e: &'tcx Expr<'_>) {
        // If this expression has an effective type of `&dyn Any` ...
        {
            let coerced_ty = cx.typeck_results().expr_ty_adjusted(e);

            let ty::Ref(_, coerced_ref_ty, _) = *coerced_ty.kind() else {
                return;
            };
            if !is_dyn_any(cx.tcx, coerced_ref_ty) {
                return;
            }
        }

        let expr_ty = cx.typeck_results().expr_ty(e);
        let ty::Ref(_, expr_ref_ty, _) = *expr_ty.kind() else {
            return;
        };
        // ... but only due to coercion ...
        if is_dyn_any(cx.tcx, expr_ref_ty) {
            return;
        }
        // ... and it also *derefs* to `dyn Any` ...
        let Some((depth, target)) = clippy_utils::ty::deref_chain(cx, expr_ref_ty).enumerate().last() else {
            return;
        };
        if !is_dyn_any(cx.tcx, target) {
            return;
        }

        // ... that's probably not intended.
        let (span, deref_count) = match e.kind {
            // If `e` was already a reference, skip `*&` in the suggestion
            ExprKind::AddrOf(_, _, referent) => (referent.span, depth),
            _ => (e.span, depth + 1),
        };
        span_lint_and_sugg(
            cx,
            COERCE_ANY_REF_TO_ANY,
            e.span,
            format!("coercing `{expr_ty}` to `&dyn Any` rather than dereferencing to the `dyn Any` inside"),
            "consider dereferencing",
            format!("&{}{}", str::repeat("*", deref_count), snippet(cx, span, "x")),
            Applicability::MaybeIncorrect,
        );
    }
}

fn is_dyn_any(tcx: TyCtxt<'_>, ty: Ty<'_>) -> bool {
    let ty::Dynamic(traits, ..) = ty.kind() else {
        return false;
    };
    traits.iter().any(|binder| {
        let Some(ExistentialPredicate::Trait(t)) = binder.no_bound_vars() else {
            return false;
        };
        tcx.is_diagnostic_item(sym::Any, t.def_id)
    })
}

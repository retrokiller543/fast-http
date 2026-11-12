use proc_macro2::{Spacing, TokenStream, TokenTree};
use syn::{
    Arm, Block, Expr, ExprClosure, ExprForLoop, ExprIf, ExprLet, ExprPath, ExprWhile, Ident, Local,
    Macro, Pat, Token,
    parse::{ParseStream, Parser},
    punctuated::Punctuated,
    visit::{self, Visit},
};

use super::bindings;

/// The names `expr` reads from outside itself: plain identifiers that no pattern inside the
/// expression binds.
///
/// Names bound by closure parameters, `match` arms, `if let`, `while let`, `let` statements,
/// `for` loops and the pattern of `matches!` shadow outer ones within their scope. Other macros
/// are read as comma-separated expressions; when their arguments are not expressions, their
/// tokens are scanned instead, skipping names after `.` or `::` and field names before a lone
/// `:`.
pub fn free_names(expr: &Expr) -> Vec<Ident> {
    let mut walker = Free::default();

    walker.visit_expr(expr);

    walker.found
}

/// Walks an expression, keeping the names bound so far in scope.
#[derive(Default)]
struct Free {
    bound: Vec<Ident>,
    found: Vec<Ident>,
}

impl Free {
    fn note(&mut self, ident: &Ident) {
        if !self.bound.contains(ident) && !self.found.contains(ident) {
            self.found.push(ident.clone());
        }
    }

    fn bind(&mut self, pat: &Pat) {
        self.bound.extend(bindings(pat));
    }

    /// Runs `body` in a scope of its own, so the names it binds go out of scope afterwards.
    fn scoped(&mut self, body: impl FnOnce(&mut Self)) {
        let depth = self.bound.len();

        body(self);
        self.bound.truncate(depth);
    }

    fn matches_macro(&mut self, input: ParseStream) -> syn::Result<()> {
        let scrutinee: Expr = input.parse()?;

        input.parse::<Token![,]>()?;

        let pat = Pat::parse_multi_with_leading_vert(input)?;
        let guard = match input.parse::<Option<Token![if]>>()? {
            Some(_) => Some(input.parse::<Expr>()?),
            None => None,
        };

        input.parse::<Option<Token![,]>>()?;
        self.visit_expr(&scrutinee);
        self.scoped(|walker| {
            walker.bind(&pat);

            if let Some(guard) = &guard {
                walker.visit_expr(guard);
            }
        });

        Ok(())
    }

    fn scan(&mut self, tokens: TokenStream) {
        let tokens: Vec<_> = tokens.into_iter().collect();

        for (index, token) in tokens.iter().enumerate() {
            match token {
                TokenTree::Ident(ident)
                    if !is_member(&tokens[..index]) && !is_field_name(tokens.get(index + 1)) =>
                {
                    self.note(ident);
                }
                TokenTree::Group(group) => self.scan(group.stream()),
                _ => {}
            }
        }
    }
}

impl<'ast> Visit<'ast> for Free {
    fn visit_expr_path(&mut self, path: &'ast ExprPath) {
        if path.qself.is_none()
            && let Some(ident) = path.path.get_ident()
        {
            self.note(ident);
        }
    }

    fn visit_expr_closure(&mut self, closure: &'ast ExprClosure) {
        self.scoped(|walker| {
            closure.inputs.iter().for_each(|input| walker.bind(input));
            walker.visit_expr(&closure.body);
        });
    }

    fn visit_arm(&mut self, arm: &'ast Arm) {
        self.scoped(|walker| {
            walker.bind(&arm.pat);

            if let Some((_, guard)) = &arm.guard {
                walker.visit_expr(guard);
            }

            walker.visit_expr(&arm.body);
        });
    }

    fn visit_expr_let(&mut self, expr: &'ast ExprLet) {
        self.visit_expr(&expr.expr);
        self.bind(&expr.pat);
    }

    fn visit_expr_if(&mut self, expr: &'ast ExprIf) {
        self.scoped(|walker| {
            walker.visit_expr(&expr.cond);
            walker.visit_block(&expr.then_branch);
        });

        if let Some((_, otherwise)) = &expr.else_branch {
            self.visit_expr(otherwise);
        }
    }

    fn visit_expr_while(&mut self, expr: &'ast ExprWhile) {
        self.scoped(|walker| {
            walker.visit_expr(&expr.cond);
            walker.visit_block(&expr.body);
        });
    }

    fn visit_expr_for_loop(&mut self, expr: &'ast ExprForLoop) {
        self.visit_expr(&expr.expr);
        self.scoped(|walker| {
            walker.bind(&expr.pat);
            walker.visit_block(&expr.body);
        });
    }

    fn visit_block(&mut self, block: &'ast Block) {
        self.scoped(|walker| visit::visit_block(walker, block));
    }

    fn visit_local(&mut self, local: &'ast Local) {
        if let Some(init) = &local.init {
            self.visit_expr(&init.expr);

            if let Some((_, diverge)) = &init.diverge {
                self.visit_expr(diverge);
            }
        }

        self.bind(&local.pat);
    }

    fn visit_macro(&mut self, mac: &'ast Macro) {
        if mac.path.is_ident("matches")
            && (|input: ParseStream| self.matches_macro(input))
                .parse2(mac.tokens.clone())
                .is_ok()
        {
            return;
        }

        match Punctuated::<Expr, Token![,]>::parse_terminated.parse2(mac.tokens.clone()) {
            Ok(args) => args.iter().for_each(|arg| self.visit_expr(arg)),
            Err(_) => self.scan(mac.tokens.clone()),
        }
    }
}

/// Whether the tokens before a name make it a member or path segment: they end in a `.` that is
/// not part of `..`, or in `::`.
fn is_member(before: &[TokenTree]) -> bool {
    let punct = |token: Option<&TokenTree>, ch: char| matches!(token, Some(TokenTree::Punct(punct)) if punct.as_char() == ch);
    let joint = |token: Option<&TokenTree>, ch: char| {
        matches!(
            token,
            Some(TokenTree::Punct(punct)) if punct.as_char() == ch && punct.spacing() == Spacing::Joint
        )
    };
    let last = before.last();
    let previous = before
        .len()
        .checked_sub(2)
        .and_then(|index| before.get(index));

    (punct(last, '.') && !joint(previous, '.')) || (punct(last, ':') && joint(previous, ':'))
}

/// Whether the token after a name makes it a field name: a `:` that does not start `::`.
fn is_field_name(next: Option<&TokenTree>) -> bool {
    matches!(
        next,
        Some(TokenTree::Punct(punct)) if punct.as_char() == ':' && punct.spacing() == Spacing::Alone
    )
}

#[cfg(test)]
mod tests;

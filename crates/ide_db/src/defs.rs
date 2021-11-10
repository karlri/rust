//! `NameDefinition` keeps information about the element we want to search references for.
//! The element is represented by `NameKind`. It's located inside some `container` and
//! has a `visibility`, which defines a search scope.
//! Note that the reference search is possible for not all of the classified items.

// FIXME: this badly needs rename/rewrite (matklad, 2020-02-06).

use arrayvec::ArrayVec;
use hir::{
    Adt, AsAssocItem, AssocItem, BuiltinType, Const, Field, Function, GenericParam, HasVisibility,
    Impl, ItemInNs, Label, Local, MacroDef, Module, ModuleDef, Name, PathResolution, Semantics,
    Static, Trait, TypeAlias, Variant, Visibility,
};
use stdx::impl_from;
use syntax::{
    ast::{self, AstNode},
    match_ast, AstToken, SyntaxKind, SyntaxNode, SyntaxToken,
};

use crate::{helpers::try_resolve_derive_input, RootDatabase};

// FIXME: a more precise name would probably be `Symbol`?
#[derive(Debug, PartialEq, Eq, Copy, Clone, Hash)]
pub enum Definition {
    Macro(MacroDef),
    Field(Field),
    Module(Module),
    Function(Function),
    Adt(Adt),
    Variant(Variant),
    Const(Const),
    Static(Static),
    Trait(Trait),
    TypeAlias(TypeAlias),
    BuiltinType(BuiltinType),
    SelfType(Impl),
    Local(Local),
    GenericParam(GenericParam),
    Label(Label),
}

impl Definition {
    pub fn from_token(
        sema: &Semantics<RootDatabase>,
        token: &SyntaxToken,
    ) -> ArrayVec<Definition, 2> {
        let parent = match token.parent() {
            Some(parent) => parent,
            None => return Default::default(),
        };
        if let Some(ident) = ast::Ident::cast(token.clone()) {
            let attr = parent
                .ancestors()
                .find_map(ast::TokenTree::cast)
                .and_then(|tt| tt.parent_meta())
                .and_then(|meta| meta.parent_attr());
            if let Some(attr) = attr {
                return try_resolve_derive_input(&sema, &attr, &ident)
                    .map(Into::into)
                    .into_iter()
                    .collect();
            }
        }
        Self::from_node(sema, &parent)
    }

    pub fn from_node(sema: &Semantics<RootDatabase>, node: &SyntaxNode) -> ArrayVec<Definition, 2> {
        let mut res = ArrayVec::new();
        (|| {
            match_ast! {
                match node {
                    ast::Name(name) => {
                        match NameClass::classify(&sema, &name)? {
                            NameClass::Definition(it) | NameClass::ConstReference(it) => res.push(it),
                            NameClass::PatFieldShorthand { local_def, field_ref } => {
                                res.push(Definition::Local(local_def));
                                res.push(Definition::Field(field_ref));
                            }
                        }
                    },
                    ast::NameRef(name_ref) => {
                        match NameRefClass::classify(sema, &name_ref)? {
                            NameRefClass::Definition(it) => res.push(it),
                            NameRefClass::FieldShorthand { local_ref, field_ref } => {
                                res.push(Definition::Local(local_ref));
                                res.push(Definition::Field(field_ref));
                            }
                        }
                    },
                    ast::Lifetime(lifetime) => {
                        let def = if let Some(x) = NameClass::classify_lifetime(&sema, &lifetime) {
                            NameClass::defined(x)
                        } else {
                            NameRefClass::classify_lifetime(&sema, &lifetime).and_then(|class| match class {
                                NameRefClass::Definition(it) => Some(it),
                                _ => None,
                            })
                        };
                        if let Some(def) = def {
                            res.push(def);
                        }
                    },
                    _ => (),
                }
            }
            Some(())
        })();
        res
    }

    pub fn canonical_module_path(&self, db: &RootDatabase) -> Option<impl Iterator<Item = Module>> {
        self.module(db).map(|it| it.path_to_root(db).into_iter().rev())
    }

    pub fn module(&self, db: &RootDatabase) -> Option<Module> {
        let module = match self {
            Definition::Macro(it) => it.module(db)?,
            Definition::Module(it) => it.parent(db)?,
            Definition::Field(it) => it.parent_def(db).module(db),
            Definition::Function(it) => it.module(db),
            Definition::Adt(it) => it.module(db),
            Definition::Const(it) => it.module(db),
            Definition::Static(it) => it.module(db),
            Definition::Trait(it) => it.module(db),
            Definition::TypeAlias(it) => it.module(db),
            Definition::Variant(it) => it.module(db),
            Definition::SelfType(it) => it.module(db),
            Definition::Local(it) => it.module(db),
            Definition::GenericParam(it) => it.module(db),
            Definition::Label(it) => it.module(db),
            Definition::BuiltinType(_) => return None,
        };
        Some(module)
    }

    pub fn visibility(&self, db: &RootDatabase) -> Option<Visibility> {
        let vis = match self {
            Definition::Field(sf) => sf.visibility(db),
            Definition::Module(it) => it.visibility(db),
            Definition::Function(it) => it.visibility(db),
            Definition::Adt(it) => it.visibility(db),
            Definition::Const(it) => it.visibility(db),
            Definition::Static(it) => it.visibility(db),
            Definition::Trait(it) => it.visibility(db),
            Definition::TypeAlias(it) => it.visibility(db),
            Definition::Variant(it) => it.visibility(db),
            Definition::BuiltinType(_) => Visibility::Public,
            Definition::Macro(_) => return None,
            Definition::SelfType(_)
            | Definition::Local(_)
            | Definition::GenericParam(_)
            | Definition::Label(_) => return None,
        };
        Some(vis)
    }

    pub fn name(&self, db: &RootDatabase) -> Option<Name> {
        let name = match self {
            Definition::Macro(it) => it.name(db)?,
            Definition::Field(it) => it.name(db),
            Definition::Module(it) => it.name(db)?,
            Definition::Function(it) => it.name(db),
            Definition::Adt(it) => it.name(db),
            Definition::Variant(it) => it.name(db),
            Definition::Const(it) => it.name(db)?,
            Definition::Static(it) => it.name(db),
            Definition::Trait(it) => it.name(db),
            Definition::TypeAlias(it) => it.name(db),
            Definition::BuiltinType(it) => it.name(),
            Definition::SelfType(_) => return None,
            Definition::Local(it) => it.name(db)?,
            Definition::GenericParam(it) => it.name(db),
            Definition::Label(it) => it.name(db),
        };
        Some(name)
    }
}

/// On a first blush, a single `ast::Name` defines a single definition at some
/// scope. That is, that, by just looking at the syntactical category, we can
/// unambiguously define the semantic category.
///
/// Sadly, that's not 100% true, there are special cases. To make sure that
/// callers handle all the special cases correctly via exhaustive matching, we
/// add a [`NameClass`] enum which lists all of them!
///
/// A model special case is `None` constant in pattern.
#[derive(Debug)]
pub enum NameClass {
    Definition(Definition),
    /// `None` in `if let None = Some(82) {}`.
    /// Syntactically, it is a name, but semantically it is a reference.
    ConstReference(Definition),
    /// `field` in `if let Foo { field } = foo`. Here, `ast::Name` both introduces
    /// a definition into a local scope, and refers to an existing definition.
    PatFieldShorthand {
        local_def: Local,
        field_ref: Field,
    },
}

impl NameClass {
    /// `Definition` defined by this name.
    pub fn defined(self) -> Option<Definition> {
        let res = match self {
            NameClass::Definition(it) => it,
            NameClass::ConstReference(_) => return None,
            NameClass::PatFieldShorthand { local_def, field_ref: _ } => {
                Definition::Local(local_def)
            }
        };
        Some(res)
    }

    pub fn classify(sema: &Semantics<RootDatabase>, name: &ast::Name) -> Option<NameClass> {
        let _p = profile::span("classify_name");

        let parent = name.syntax().parent()?;

        if let Some(bind_pat) = ast::IdentPat::cast(parent.clone()) {
            if let Some(def) = sema.resolve_bind_pat_to_const(&bind_pat) {
                return Some(NameClass::ConstReference(Definition::from(def)));
            }
        }

        match_ast! {
            match parent {
                ast::Rename(it) => {
                    if let Some(use_tree) = it.syntax().parent().and_then(ast::UseTree::cast) {
                        let path = use_tree.path()?;
                        let path_segment = path.segment()?;
                        let name_ref = path_segment.name_ref()?;
                        let name_ref = if name_ref.self_token().is_some() {
                             use_tree
                                .syntax()
                                .parent()
                                .as_ref()
                                // Skip over UseTreeList
                                .and_then(|it| {
                                    let use_tree = it.parent().and_then(ast::UseTree::cast)?;
                                    let path = use_tree.path()?;
                                    let path_segment = path.segment()?;
                                    path_segment.name_ref()
                                }).unwrap_or(name_ref)
                        } else {
                            name_ref
                        };
                        let name_ref_class = NameRefClass::classify(sema, &name_ref)?;

                        Some(NameClass::Definition(match name_ref_class {
                            NameRefClass::Definition(def) => def,
                            NameRefClass::FieldShorthand { local_ref: _, field_ref } => {
                                Definition::Field(field_ref)
                            }
                        }))
                    } else {
                        let extern_crate = it.syntax().parent().and_then(ast::ExternCrate::cast)?;
                        let krate = sema.resolve_extern_crate(&extern_crate)?;
                        let root_module = krate.root_module(sema.db);
                        Some(NameClass::Definition(Definition::Module(root_module)))
                    }
                },
                ast::IdentPat(it) => {
                    let local = sema.to_def(&it)?;

                    if let Some(record_pat_field) = it.syntax().parent().and_then(ast::RecordPatField::cast) {
                        if record_pat_field.name_ref().is_none() {
                            if let Some(field) = sema.resolve_record_pat_field(&record_pat_field) {
                                return Some(NameClass::PatFieldShorthand { local_def: local, field_ref: field });
                            }
                        }
                    }

                    Some(NameClass::Definition(Definition::Local(local)))
                },
                ast::SelfParam(it) => {
                    let def = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Local(def)))
                },
                ast::RecordField(it) => {
                    let field: hir::Field = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Field(field)))
                },
                ast::Module(it) => {
                    let def = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Module(def)))
                },
                ast::Struct(it) => {
                    let def: hir::Struct = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Adt(def.into())))
                },
                ast::Union(it) => {
                    let def: hir::Union = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Adt(def.into())))
                },
                ast::Enum(it) => {
                    let def: hir::Enum = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Adt(def.into())))
                },
                ast::Trait(it) => {
                    let def: hir::Trait = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Trait(def)))
                },
                ast::Static(it) => {
                    let def: hir::Static = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Static(def)))
                },
                ast::Variant(it) => {
                    let def: hir::Variant = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Variant(def)))
                },
                ast::Fn(it) => {
                    let def: hir::Function = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Function(def)))
                },
                ast::Const(it) => {
                    let def: hir::Const = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Const(def)))
                },
                ast::TypeAlias(it) => {
                    let def: hir::TypeAlias = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::TypeAlias(def)))
                },
                ast::Macro(it) => {
                    let def = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Macro(def)))
                },
                ast::TypeParam(it) => {
                    let def = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::GenericParam(def.into())))
                },
                ast::ConstParam(it) => {
                    let def = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::GenericParam(def.into())))
                },
                _ => None,
            }
        }
    }

    pub fn classify_lifetime(
        sema: &Semantics<RootDatabase>,
        lifetime: &ast::Lifetime,
    ) -> Option<NameClass> {
        let _p = profile::span("classify_lifetime").detail(|| lifetime.to_string());
        let parent = lifetime.syntax().parent()?;

        match_ast! {
            match parent {
                ast::LifetimeParam(it) => {
                    let def = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::GenericParam(def.into())))
                },
                ast::Label(it) => {
                    let def = sema.to_def(&it)?;
                    Some(NameClass::Definition(Definition::Label(def)))
                },
                _ => None,
            }
        }
    }
}

/// This is similar to [`NameClass`], but works for [`ast::NameRef`] rather than
/// for [`ast::Name`]. Similarly, what looks like a reference in syntax is a
/// reference most of the time, but there are a couple of annoying exceptions.
///
/// A model special case is field shorthand syntax, which uses a single
/// reference to point to two different defs.
#[derive(Debug)]
pub enum NameRefClass {
    Definition(Definition),
    FieldShorthand { local_ref: Local, field_ref: Field },
}

impl NameRefClass {
    // Note: we don't have unit-tests for this rather important function.
    // It is primarily exercised via goto definition tests in `ide`.
    pub fn classify(
        sema: &Semantics<RootDatabase>,
        name_ref: &ast::NameRef,
    ) -> Option<NameRefClass> {
        let _p = profile::span("classify_name_ref").detail(|| name_ref.to_string());

        let parent = name_ref.syntax().parent()?;

        if let Some(method_call) = ast::MethodCallExpr::cast(parent.clone()) {
            if let Some(func) = sema.resolve_method_call(&method_call) {
                return Some(NameRefClass::Definition(Definition::Function(func)));
            }
        }

        if let Some(field_expr) = ast::FieldExpr::cast(parent.clone()) {
            if let Some(field) = sema.resolve_field(&field_expr) {
                return Some(NameRefClass::Definition(Definition::Field(field)));
            }
        }

        if let Some(record_field) = ast::RecordExprField::for_field_name(name_ref) {
            if let Some((field, local, _)) = sema.resolve_record_field(&record_field) {
                let res = match local {
                    None => NameRefClass::Definition(Definition::Field(field)),
                    Some(local) => {
                        NameRefClass::FieldShorthand { field_ref: field, local_ref: local }
                    }
                };
                return Some(res);
            }
        }

        if let Some(record_pat_field) = ast::RecordPatField::cast(parent.clone()) {
            if let Some(field) = sema.resolve_record_pat_field(&record_pat_field) {
                let field = Definition::Field(field);
                return Some(NameRefClass::Definition(field));
            }
        }

        if let Some(assoc_type_arg) = ast::AssocTypeArg::cast(parent.clone()) {
            if assoc_type_arg.name_ref().as_ref() == Some(name_ref) {
                // `Trait<Assoc = Ty>`
                //        ^^^^^
                let path = name_ref.syntax().ancestors().find_map(ast::Path::cast)?;
                let resolved = sema.resolve_path(&path)?;
                if let PathResolution::Def(ModuleDef::Trait(tr)) = resolved {
                    // FIXME: resolve in supertraits
                    if let Some(ty) = tr
                        .items(sema.db)
                        .iter()
                        .filter_map(|assoc| match assoc {
                            hir::AssocItem::TypeAlias(it) => Some(*it),
                            _ => None,
                        })
                        .find(|alias| alias.name(sema.db).to_smol_str() == name_ref.text().as_str())
                    {
                        return Some(NameRefClass::Definition(Definition::TypeAlias(ty)));
                    }
                }

                return None;
            }
        }

        if let Some(path) = name_ref.syntax().ancestors().find_map(ast::Path::cast) {
            if path.qualifier().is_none() {
                if let Some(macro_call) = path.syntax().parent().and_then(ast::MacroCall::cast) {
                    // Only use this to resolve single-segment macro calls like `foo!()`. Multi-segment
                    // paths are handled below (allowing `log$0::info!` to resolve to the log crate).
                    if let Some(macro_def) = sema.resolve_macro_call(&macro_call) {
                        return Some(NameRefClass::Definition(Definition::Macro(macro_def)));
                    }
                }
            }
            let top_path = path.top_path();
            let is_attribute_path = top_path
                .syntax()
                .ancestors()
                .find_map(ast::Attr::cast)
                .map(|attr| attr.path().as_ref() == Some(&top_path));
            return match is_attribute_path {
                Some(true) if path == top_path => sema
                    .resolve_path_as_macro(&path)
                    .filter(|mac| mac.kind() == hir::MacroKind::Attr)
                    .map(Definition::Macro)
                    .map(NameRefClass::Definition),
                // in case of the path being a qualifier, don't resolve to anything but a module
                Some(true) => match sema.resolve_path(&path)? {
                    PathResolution::Def(ModuleDef::Module(module)) => {
                        cov_mark::hit!(name_ref_classify_attr_path_qualifier);
                        Some(NameRefClass::Definition(Definition::Module(module)))
                    }
                    _ => None,
                },
                // inside attribute, but our path isn't part of the attribute's path(might be in its expression only)
                Some(false) => None,
                None => sema.resolve_path(&path).map(Into::into).map(NameRefClass::Definition),
            };
        }

        let extern_crate = ast::ExternCrate::cast(parent)?;
        let krate = sema.resolve_extern_crate(&extern_crate)?;
        let root_module = krate.root_module(sema.db);
        Some(NameRefClass::Definition(Definition::Module(root_module)))
    }

    pub fn classify_lifetime(
        sema: &Semantics<RootDatabase>,
        lifetime: &ast::Lifetime,
    ) -> Option<NameRefClass> {
        let _p = profile::span("classify_lifetime_ref").detail(|| lifetime.to_string());
        let parent = lifetime.syntax().parent()?;
        match parent.kind() {
            SyntaxKind::BREAK_EXPR | SyntaxKind::CONTINUE_EXPR => {
                sema.resolve_label(lifetime).map(Definition::Label).map(NameRefClass::Definition)
            }
            SyntaxKind::LIFETIME_ARG
            | SyntaxKind::SELF_PARAM
            | SyntaxKind::TYPE_BOUND
            | SyntaxKind::WHERE_PRED
            | SyntaxKind::REF_TYPE => sema
                .resolve_lifetime_param(lifetime)
                .map(GenericParam::LifetimeParam)
                .map(Definition::GenericParam)
                .map(NameRefClass::Definition),
            // lifetime bounds, as in the 'b in 'a: 'b aren't wrapped in TypeBound nodes so we gotta check
            // if our lifetime is in a LifetimeParam without being the constrained lifetime
            _ if ast::LifetimeParam::cast(parent).and_then(|param| param.lifetime()).as_ref()
                != Some(lifetime) =>
            {
                sema.resolve_lifetime_param(lifetime)
                    .map(GenericParam::LifetimeParam)
                    .map(Definition::GenericParam)
                    .map(NameRefClass::Definition)
            }
            _ => None,
        }
    }
}

impl AsAssocItem for Definition {
    fn as_assoc_item(self, db: &dyn hir::db::HirDatabase) -> Option<AssocItem> {
        match self {
            Definition::Function(it) => it.as_assoc_item(db),
            Definition::Const(it) => it.as_assoc_item(db),
            Definition::TypeAlias(it) => it.as_assoc_item(db),
            _ => None,
        }
    }
}

impl_from!(
    Field, Module, Function, Adt, Variant, Const, Static, Trait, TypeAlias, BuiltinType, Local,
    GenericParam, Label
    for Definition
);

impl From<PathResolution> for Definition {
    fn from(path_resolution: PathResolution) -> Self {
        match path_resolution {
            PathResolution::Def(def) => def.into(),
            PathResolution::AssocItem(item) => {
                let def: ModuleDef = match item {
                    hir::AssocItem::Function(it) => it.into(),
                    hir::AssocItem::Const(it) => it.into(),
                    hir::AssocItem::TypeAlias(it) => it.into(),
                };
                def.into()
            }
            PathResolution::Local(local) => Definition::Local(local),
            PathResolution::TypeParam(par) => Definition::GenericParam(par.into()),
            PathResolution::Macro(def) => Definition::Macro(def),
            PathResolution::SelfType(impl_def) => Definition::SelfType(impl_def),
            PathResolution::ConstParam(par) => Definition::GenericParam(par.into()),
        }
    }
}

impl From<ModuleDef> for Definition {
    fn from(def: ModuleDef) -> Self {
        match def {
            ModuleDef::Module(it) => Definition::Module(it),
            ModuleDef::Function(it) => Definition::Function(it),
            ModuleDef::Adt(it) => Definition::Adt(it),
            ModuleDef::Variant(it) => Definition::Variant(it),
            ModuleDef::Const(it) => Definition::Const(it),
            ModuleDef::Static(it) => Definition::Static(it),
            ModuleDef::Trait(it) => Definition::Trait(it),
            ModuleDef::TypeAlias(it) => Definition::TypeAlias(it),
            ModuleDef::BuiltinType(it) => Definition::BuiltinType(it),
        }
    }
}

impl From<Definition> for Option<ItemInNs> {
    fn from(def: Definition) -> Self {
        let item = match def {
            Definition::Module(it) => ModuleDef::Module(it),
            Definition::Function(it) => ModuleDef::Function(it),
            Definition::Adt(it) => ModuleDef::Adt(it),
            Definition::Variant(it) => ModuleDef::Variant(it),
            Definition::Const(it) => ModuleDef::Const(it),
            Definition::Static(it) => ModuleDef::Static(it),
            Definition::Trait(it) => ModuleDef::Trait(it),
            Definition::TypeAlias(it) => ModuleDef::TypeAlias(it),
            Definition::BuiltinType(it) => ModuleDef::BuiltinType(it),
            _ => return None,
        };
        Some(ItemInNs::from(item))
    }
}

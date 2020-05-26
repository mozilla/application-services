/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::io::prelude::*;
use std::{
    env,
    collections::HashMap,
    convert::TryFrom, convert::TryInto,
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::bail;
use anyhow::Result;

#[derive(Debug, Default)]
pub struct ComponentInterface {
    //raw: String,
    pub members: Vec<InterfaceMember>,
}

#[derive(Debug)]
pub enum Literal {
    Boolean(bool),
    String(String),
    // TODO: more types of literal
}

#[derive(Debug)]
pub enum TypeReference {
    Boolean,
    String,
    U8,
    S8,
    U16,
    S16,
    U32,
    S32,
    U64,
    S64,
    Identifier(String),
    Sequence(Box<TypeReference>),
    Union(Vec<Box<TypeReference>>),
}

#[derive(Debug)]
pub enum InterfaceMember {
    Object(ObjectType),
    Record(RecordType),
    Enum(EnumType),
}

#[derive(Debug, Default)]
pub struct ObjectType {
    pub name: String, // could probably borrow this from the container in some clever fashion, but whatevz...
    pub members: Vec<ObjectTypeMember>,
}

#[derive(Debug)]
pub enum ObjectTypeMember {
    Constructor(ObjectTypeConstructor),
    Method(ObjectTypeMethod)
}

#[derive(Debug)]
pub struct ObjectTypeConstructor {
    pub argument_types: Vec<ObjectTypeArgument>,
}

impl ObjectTypeConstructor {
    pub fn ffi_name(&self) -> String {
        "fxa_new".to_string() // XXX TODO: calculate prefix from the containing Object declaration, somehow.
    }
}

#[derive(Debug)]
pub struct ObjectTypeMethod {
    pub name: String,
    pub return_type: Option<TypeReference>,
    pub argument_types: Vec<ObjectTypeArgument>,
}


impl ObjectTypeMethod {
    pub fn ffi_name(&self) -> String {
        let mut nm = String::from("fxa_"); // XXX TODO: calculate prefix from the containing Object declaration, somehow.
        nm.push_str(&self.name);
        nm
    }
}

#[derive(Debug)]
pub struct ObjectTypeArgument {
    pub name: String,
    pub typ: TypeReference,
    pub optional: bool,
    pub default: Option<Literal>,
}

#[derive(Debug, Default)]
pub struct RecordType {
    pub name: String,
    pub fields: Vec<RecordTypeField>,
}

#[derive(Debug)]
pub struct RecordTypeField {
    pub name: String,
    pub typ: TypeReference,
    pub required: bool,
    pub default: Option<Literal>,
}

#[derive(Debug, Default)]
pub struct EnumType {
    pub name: String,
    pub values: Vec<String>,
}

impl ComponentInterface {

    pub fn new_from_str(idl: &str) -> Result<Self> {
        let mut component = Self::default();
        // There's some lifetime thing with the errors returned from weedle::parse
        // that life is too short to figure out; unwrap and move on.
        for defn in weedle::parse(idl.trim()).unwrap() {
            component.add_type_definition(defn.try_into()?);
        }
        Ok(component)
    }

    fn add_type_definition(&mut self, typ: InterfaceMember) {
        self.members.push(typ)
    }
}

impl InterfaceMember {
    fn name(&self) -> &str{
        match self {
            InterfaceMember::Object(t) => &t.name,
            InterfaceMember::Record(t) => &t.name,
            InterfaceMember::Enum(t) => &t.name,
        }
    }
}

impl TryFrom<weedle::Definition<'_>> for InterfaceMember {
    type Error = anyhow::Error;
    fn try_from(d: weedle::Definition) -> Result<Self> {
        Ok(match d {
            weedle::Definition::Interface(d) => InterfaceMember::Object(d.try_into()?),
            weedle::Definition::Dictionary(d) => InterfaceMember::Record(d.try_into()?),
            weedle::Definition::Enum(d) => InterfaceMember::Enum(d.try_into()?),
            _ => bail!("don't know how to deal with {:?}", d),
        })
    }
}

impl TryFrom<weedle::InterfaceDefinition<'_>> for ObjectType {
    type Error = anyhow::Error;
    fn try_from(d: weedle::InterfaceDefinition) -> Result<Self> {
        if d.attributes.is_some() {
            bail!("no interface attributes are supported yet");
        }
        if d.inheritance.is_some() {
            bail!("interface inheritence is not support");
        }
        Ok(ObjectType {
            name: d.identifier.0.to_string(),
            // XXX TODO: here and elsewhere, we need some sort of `try_map` method that
            // does the same thing as `map` but bubbles up any errors to the outer function.
            // Maybe this already exists and I just don't know about it..?
            // Anyway, we panic instead for now; YOLO.
            members: d.members.body.iter().map(|v| match v {
                weedle::interface::InterfaceMember::Constructor(t) => ObjectTypeMember::Constructor(t.try_into().unwrap()),
                weedle::interface::InterfaceMember::Operation(t) => ObjectTypeMember::Method(t.try_into().unwrap()),
                _ => panic!("no support for interface member type {:?} yet", d),
            }).collect()
        })
    }
}

impl TryFrom<&weedle::interface::ConstructorInterfaceMember<'_>> for ObjectTypeConstructor {
    type Error = anyhow::Error;
    fn try_from(m: &weedle::interface::ConstructorInterfaceMember) -> Result<Self> {
        if m.attributes.is_some() {
            bail!("no interface member attributes supported yet");
        }
        Ok(ObjectTypeConstructor {
            argument_types: m.args.body.list.iter().map(|v| v.try_into().unwrap()).collect()
        })
    }
}

impl TryFrom<&weedle::interface::OperationInterfaceMember<'_>> for ObjectTypeMethod {
    type Error = anyhow::Error;
    fn try_from(m: &weedle::interface::OperationInterfaceMember) -> Result<Self> {
        if m.attributes.is_some() {
            bail!("no interface member attributes supported yet");
        }
        if m.special.is_some() {
            bail!("special operations not supported");
        }
        if let Some(weedle::interface::StringifierOrStatic::Stringifier(_)) = m.modifier {
            bail!("stringifiers are not supported");
        }
        if let None = m.identifier {
            bail!("anonymous methods are not supported {:?}", m);
        }
        Ok(ObjectTypeMethod {
            name: m.identifier.unwrap().0.to_string(),
            return_type: match &m.return_type {
                weedle::types::ReturnType::Void(_) => None,
                weedle::types::ReturnType::Type(t) => Some(t.try_into()?)
            },
            argument_types: m.args.body.list.iter().map(|v| v.try_into().unwrap()).collect()
        })
    }
}

impl TryFrom<&weedle::argument::Argument<'_>> for ObjectTypeArgument {
    type Error = anyhow::Error;
    fn try_from(t: &weedle::argument::Argument) -> Result<Self> {
        Ok(match t {
            weedle::argument::Argument::Single(t) => t.try_into()?,
            weedle::argument::Argument::Variadic(_) => bail!("variadic arguments not supported"),
        })
    }
}

impl TryFrom<&weedle::argument::SingleArgument<'_>> for ObjectTypeArgument {
    type Error = anyhow::Error;
    fn try_from(a: &weedle::argument::SingleArgument) -> Result<Self> {
        if a.attributes.is_some() {
            bail!("no argument attributes supported yet");
        }
        Ok(ObjectTypeArgument {
            name: a.identifier.0.to_string(),
            typ: (&a.type_).try_into()?,
            optional: a.optional.is_some(),
            default: a.default.map(|v| v.value.try_into().unwrap())
        })
    }
}

impl TryFrom<weedle::DictionaryDefinition<'_>> for RecordType {
    type Error = anyhow::Error;
    fn try_from(d: weedle::DictionaryDefinition) -> Result<Self> {
        if d.attributes.is_some() {
            bail!("no dictionary attributes are supported yet");
        }
        if d.inheritance.is_some() {
            bail!("dictionary inheritence is not support");
        }
        Ok(RecordType {
            name: d.identifier.0.to_string(),
            fields: d.members.body.iter().map(|f| {
                f.try_into().unwrap()
            }).collect()

        })
    }
}

impl TryFrom<weedle::EnumDefinition<'_>> for EnumType {
    type Error = anyhow::Error;
    fn try_from(d: weedle::EnumDefinition) -> Result<Self> {
        if d.attributes.is_some() {
            bail!("no enum attributes are supported yet");
        }
        Ok(EnumType {
            name: d.identifier.0.to_string(),
            values: d.values.body.list.iter().map(|v| v.0.to_string()).collect(),
        })
    }
}

impl TryFrom<&weedle::dictionary::DictionaryMember<'_>> for RecordTypeField {
    type Error = anyhow::Error;
    fn try_from(d: &weedle::dictionary::DictionaryMember) -> Result<Self> {
        if d.attributes.is_some() {
            bail!("no dictionary member attributes are supported yet");
        }
        Ok(Self {
            name: d.identifier.0.to_string(),
            typ: (&d.type_).try_into()?,
            required: d.required.is_some(),
            default: d.default.map(|v| v.value.try_into().unwrap())
        })
    }
}

impl TryFrom<&weedle::types::Type<'_>> for TypeReference {
    type Error = anyhow::Error;
    fn try_from(t: &weedle::types::Type) -> Result<Self> {
        Ok(match t {
            weedle::types::Type::Single(t) => {
                match t {
                    weedle::types::SingleType::Any(_) => bail!("no support for `any` types"),
                    weedle::types::SingleType::NonAny(t) => t.try_into()?,
                }
            },
            weedle::types::Type::Union(t) => {
                if t.q_mark.is_some() {
                    bail!("no support for nullable types in unions yet");
                }
                TypeReference::Union(t.type_.body.list.iter().map(|v| Box::new(match v {
                    weedle::types::UnionMemberType::Single(t) => {
                        t.try_into().unwrap()
                    },
                    weedle::types::UnionMemberType::Union(t) => panic!("no support for union union member types yet"),
                })).collect())
            },
        })
    }
}

impl TryFrom<weedle::types::NonAnyType<'_>> for TypeReference {
    type Error = anyhow::Error;
    fn try_from(t: weedle::types::NonAnyType) -> Result<Self> {
        (&t).try_into()
    }
}

impl TryFrom<&weedle::types::NonAnyType<'_>> for TypeReference {
    type Error = anyhow::Error;
    fn try_from(t: &weedle::types::NonAnyType) -> Result<Self> {
        Ok(match t {
            weedle::types::NonAnyType::Boolean(t) => t.try_into()?,
            weedle::types::NonAnyType::Identifier(t) => t.try_into()?,
            weedle::types::NonAnyType::Integer(t) => t.try_into()?,
            weedle::types::NonAnyType::Sequence(t) => t.try_into()?,
            _ => bail!("no support for type reference {:?}", t),
        })
    }
}

impl TryFrom<&weedle::types::AttributedNonAnyType<'_>> for TypeReference {
    type Error = anyhow::Error;
    fn try_from(t: &weedle::types::AttributedNonAnyType) -> Result<Self> {
        if t.attributes.is_some() {
            bail!("type attributes no support yet");
        }
        (&t.type_).try_into()
    }
}

impl TryFrom<&weedle::types::AttributedType<'_>> for TypeReference {
    type Error = anyhow::Error;
    fn try_from(t: &weedle::types::AttributedType) -> Result<Self> {
        if t.attributes.is_some() {
            bail!("type attributes no support yet");
        }
        (&t.type_).try_into()
    }
}

// The `Clone` bound here is because I don't know enough about the typesystem
// to know of to make this generic over T when T has lifetimes involved.
impl <T: TryInto<TypeReference, Error=anyhow::Error> + Clone> TryFrom<&weedle::types::MayBeNull<T>> for TypeReference {
    type Error = anyhow::Error;
    fn try_from(t: &weedle::types::MayBeNull<T>) -> Result<Self> {
        if t.q_mark.is_some() {
            bail!("no support for nullable types yet");
        }
        TryInto::try_into(t.type_.clone())
    }
}

impl TryFrom<weedle::types::IntegerType> for TypeReference {
    type Error = anyhow::Error;
    fn try_from(t: weedle::types::IntegerType) -> Result<Self> {
        bail!("integer types not implemented ({:?}); consider using u8, u16, u32 or u64", t)
    }
}

impl TryFrom<weedle::term::Boolean> for TypeReference {
    type Error = anyhow::Error;
    fn try_from(t: weedle::term::Boolean) -> Result<Self> {
        Ok(TypeReference::Boolean)
    }
}

impl TryFrom<weedle::types::SequenceType<'_>> for TypeReference {
    type Error = anyhow::Error;
    fn try_from(t: weedle::types::SequenceType) -> Result<Self> {
        Ok(TypeReference::Sequence(Box::new(t.generics.body.as_ref().try_into()?)))
    }
}

impl TryFrom<weedle::common::Identifier<'_>> for TypeReference {
    type Error = anyhow::Error;
    fn try_from(t: weedle::common::Identifier) -> Result<Self> {
        // Hard-code a couple of our own non-WebIDL-standard type names.
        Ok(match t.0.as_ref() {
            "string" => TypeReference::String,
            "u8" => TypeReference::U8,
            "s8" => TypeReference::S8,
            "u16" => TypeReference::U16,
            "s16" => TypeReference::S16,
            "u32" => TypeReference::U32,
            "s32" => TypeReference::S32,
            "u64" => TypeReference::U64,
            "s64" => TypeReference::S64,
            _ => TypeReference::Identifier(t.0.to_string())
        })
    }
}

impl TryFrom<weedle::literal::DefaultValue<'_>> for Literal {
    type Error = anyhow::Error;
    fn try_from(v: weedle::literal::DefaultValue) -> Result<Self> {
        Ok(match v {
            weedle::literal::DefaultValue::Boolean(b) => Literal::Boolean(b.0),
            weedle::literal::DefaultValue::String(s) => Literal::String(s.0.to_string()),
            _ => bail!("no support for {:?} literal yet", v),
        })
    }
}
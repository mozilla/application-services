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
struct ComponentInterface {
    //raw: String,
    types: HashMap<String, InterfaceType>,
}

#[derive(Debug)]
enum Literal {
    Boolean(bool),
    String(String),
    // TODO: more types of literal
}

#[derive(Debug)]
enum TypeReference {
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
enum InterfaceType {
    Object(ObjectType),
    Record(RecordType),
    Enum(EnumType),
}

#[derive(Debug, Default)]
struct ObjectType {
    name: String, // could probably borrow this from the container in some clever fashion, but whatevz...
    members: Vec<ObjectTypeMember>,
}

#[derive(Debug)]
enum ObjectTypeMember {
    Constructor(ObjectTypeConstructor),
    Method(ObjectTypeMethod)
}

#[derive(Debug)]
struct ObjectTypeConstructor {
    argument_types: Vec<ObjectTypeArgument>,
}

#[derive(Debug)]
struct ObjectTypeMethod {
    name: String,
    return_type: Option<TypeReference>,
    argument_types: Vec<ObjectTypeArgument>,
}

#[derive(Debug)]
struct ObjectTypeArgument {
    name: String,
    typ: TypeReference,
    optional: bool,
    default: Option<Literal>,
}

#[derive(Debug, Default)]
struct RecordType {
    name: String,
    fields: Vec<RecordTypeField>,
}

#[derive(Debug)]
struct RecordTypeField {
    name: String,
    typ: TypeReference,
    required: bool,
    default: Option<Literal>,
}

#[derive(Debug, Default)]
struct EnumType {
    name: String,
    values: Vec<String>,
}

impl ComponentInterface {
    pub fn from_weedle(defns: weedle::Definitions) -> Result<Self> {
        let mut interface = Self::default();
        for defn in defns {
            interface.add_type_definition(defn.try_into()?)?;
        }
        Ok(interface)
    }

    fn add_type_definition(&mut self, typ: InterfaceType) -> Result<()> {
        match self.types.insert(typ.name().to_string(), typ) {
            Some(typ) => bail!("duplicate definition for name \"{}\"", typ.name()),
            None => {},
        }
        Ok(())
    }
}

impl InterfaceType {
    fn name(&self) -> &str{
        match self {
            InterfaceType::Object(t) => &t.name,
            InterfaceType::Record(t) => &t.name,
            InterfaceType::Enum(t) => &t.name,
        }
    }
}

impl TryFrom<weedle::Definition<'_>> for InterfaceType {
    type Error = anyhow::Error;
    fn try_from(d: weedle::Definition) -> Result<Self> {
        Ok(match d {
            weedle::Definition::Interface(d) => InterfaceType::Object(d.try_into()?),
            weedle::Definition::Dictionary(d) => InterfaceType::Record(d.try_into()?),
            weedle::Definition::Enum(d) => InterfaceType::Enum(d.try_into()?),
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

pub fn generate_component_scaffolding(idl_file: &str) {
    println!("cargo:rerun-if-changed={}", idl_file);
    let parsed = parse(idl_file);
    // XXX TODO: give the output file a unique name related to the input file.
    let mut filename = Path::new(idl_file).file_stem().unwrap().to_os_string();
    filename.push(".uniffi.rs");
    let mut out_file = PathBuf::from(env::var("OUT_DIR").unwrap());
    out_file.push(filename);
    let mut f = File::create(out_file).unwrap();
    write!(f, "{:?}", parsed).unwrap();
}

fn parse(idl_file: &str) -> Result<ComponentInterface> {
    let mut idl = String::new();
    let mut f = File::open(idl_file)?;
    f.read_to_string(&mut idl)?;
    // XXX TODO: I think the error here needs a lifetime greater than `idl`; unwrap() it for now.
    let parsed = weedle::parse(&idl.trim()).unwrap();
    ComponentInterface::from_weedle(parsed)
}
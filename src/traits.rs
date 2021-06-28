use crate::error::*;
use crate::{Any, Class, Explicit, Implicit, Tag, TaggedValue};
use std::convert::{TryFrom, TryInto};
use std::io::Write;

/// Phantom type representing a BER parser
#[doc(hidden)]
#[derive(Debug)]
pub enum BerParser {}

/// Phantom type representing a DER parser
#[doc(hidden)]
#[derive(Debug)]
pub enum DerParser {}

#[doc(hidden)]
pub trait ASN1Parser {}

impl ASN1Parser for BerParser {}
impl ASN1Parser for DerParser {}

pub trait Tagged {
    const TAG: Tag;
}

impl<T> Tagged for &'_ T
where
    T: Tagged,
{
    const TAG: Tag = T::TAG;
}

pub trait DynTagged {
    fn tag(&self) -> Tag;
}

impl<T> DynTagged for T
where
    T: Tagged,
{
    fn tag(&self) -> Tag {
        T::TAG
    }
}

/// Base trait for BER object parsers
///
/// Library authors should usually not directly implement this trait, but should prefer implementing the
/// `TryFrom<Any>` trait,
/// which offers greater flexibility and provides an equivalent `BerParser` implementation for free.
pub trait FromBer<'a>: Sized {
    fn from_ber(bytes: &'a [u8]) -> ParseResult<'a, Self>;
}

impl<'a, T> FromBer<'a> for T
where
    T: TryFrom<Any<'a>, Error = Error>,
{
    fn from_ber(bytes: &'a [u8]) -> ParseResult<T> {
        let (i, any) = Any::from_ber(bytes)?;
        let result = any.try_into().map_err(nom::Err::Failure)?;
        Ok((i, result))
    }
}

/// Base trait for DER object parsers
///
/// Library authors should usually not directly implement this trait, but should prefer implementing the
/// `TryFrom<Any>` + `CheckDerConstraint` traits,
/// which offers greater flexibility and provides an equivalent `DerParser` implementation for free.

pub trait FromDer<'a>: Sized {
    fn from_der(bytes: &'a [u8]) -> ParseResult<'a, Self>;
}

impl<'a, T> FromDer<'a> for T
where
    T: TryFrom<Any<'a>, Error = Error>,
    T: CheckDerConstraints,
{
    fn from_der(bytes: &'a [u8]) -> ParseResult<T> {
        let (i, any) = Any::from_der(bytes)?;
        // X.690 section 10.1: definite form of length encoding shall be used
        if !any.header.length.is_definite() {
            return Err(nom::Err::Failure(Error::IndefiniteLengthUnexpected));
        }
        <T as CheckDerConstraints>::check_constraints(&any).map_err(nom::Err::Failure)?;
        let result = any.try_into().map_err(nom::Err::Failure)?;
        Ok((i, result))
    }
}

/// Verification of DER constraints
pub trait CheckDerConstraints {
    fn check_constraints(any: &Any) -> Result<()>;
}

/// Common trait for all objects that can be encoded using the DER representation
///
/// # Examples
///
/// Objects from this crate can be encoded as DER:
///
/// ```
/// use asn1_rs::{Integer, ToDer};
///
/// let int = Integer::from(4u32);
/// let mut writer = Vec::new();
/// let sz = int.write_der(&mut writer).expect("serialization failed");
///
/// assert_eq!(&writer, &[0x02, 0x01, 0x04]);
/// # assert_eq!(sz, 3);
/// ```
///
/// Many of the primitive types can also directly be encoded as DER:
///
/// ```
/// use asn1_rs::ToDer;
///
/// let mut writer = Vec::new();
/// let sz = 4.write_der(&mut writer).expect("serialization failed");
///
/// assert_eq!(&writer, &[0x02, 0x01, 0x04]);
/// # assert_eq!(sz, 3);
/// ```
pub trait ToDer
where
    Self: DynTagged,
{
    /// Get the length of the object, when encoded
    ///
    // Since we are using DER, length cannot be Indefinite, so we can use `usize`.
    // XXX can this function fail?
    fn to_der_len(&self) -> Result<usize>;

    /// Write the DER encoded representation to a newly allocated `Vec<u8>`.
    fn to_der_vec(&self) -> SerializeResult<Vec<u8>> {
        let mut v = Vec::new();
        let _ = self.write_der(&mut v)?;
        Ok(v)
    }

    /// Similar to using `to_vec`, but uses provided values without changes.
    /// This can generate an invalid encoding for a DER object.
    fn to_der_vec_raw(&self) -> SerializeResult<Vec<u8>> {
        let mut v = Vec::new();
        let _ = self.write_der_raw(&mut v)?;
        Ok(v)
    }

    /// Attempt to write the DER encoded representation (header and content) into this writer.
    ///
    /// # Examples
    ///
    /// ```
    /// use asn1_rs::{Integer, ToDer};
    ///
    /// let int = Integer::from(4u32);
    /// let mut writer = Vec::new();
    /// let sz = int.write_der(&mut writer).expect("serialization failed");
    ///
    /// assert_eq!(&writer, &[0x02, 0x01, 0x04]);
    /// # assert_eq!(sz, 3);
    /// ```
    fn write_der(&self, writer: &mut dyn Write) -> SerializeResult<usize> {
        let sz = self.write_der_header(writer)?;
        let sz = sz + self.write_der_content(writer)?;
        Ok(sz)
    }

    /// Attempt to write the DER header to this writer.
    fn write_der_header(&self, writer: &mut dyn Write) -> SerializeResult<usize>;

    /// Attempt to write the DER content (all except header) to this writer.
    fn write_der_content(&self, writer: &mut dyn Write) -> SerializeResult<usize>;

    /// Similar to using `to_der`, but uses provided values without changes.
    /// This can generate an invalid encoding for a DER object.
    fn write_der_raw(&self, writer: &mut dyn Write) -> SerializeResult<usize> {
        self.write_der(writer)
    }
}

impl<'a, T> ToDer for &'a T
where
    T: ToDer,
    &'a T: DynTagged,
{
    fn to_der_len(&self) -> Result<usize> {
        (*self).to_der_len()
    }

    fn write_der_header(&self, writer: &mut dyn Write) -> SerializeResult<usize> {
        (*self).write_der_header(writer)
    }

    fn write_der_content(&self, writer: &mut dyn Write) -> SerializeResult<usize> {
        (*self).write_der_content(writer)
    }
}

pub trait AsTaggedExplicit<'a>: Sized {
    fn explicit(self, class: Class, tag: u32) -> TaggedValue<'a, Explicit, Self> {
        TaggedValue::new_explicit(class, tag, self)
    }
}

impl<'a, T> AsTaggedExplicit<'a> for T where T: Sized + 'a {}

pub trait AsTaggedImplicit<'a>: Sized {
    fn implicit(self, class: Class, structured: u8, tag: u32) -> TaggedValue<'a, Implicit, Self> {
        TaggedValue::new_implicit(class, structured, tag, self)
    }
}

impl<'a, T> AsTaggedImplicit<'a> for T where T: Sized + 'a {}

pub trait ToStatic {
    type Owned: 'static;
    fn to_static(&self) -> Self::Owned;
}

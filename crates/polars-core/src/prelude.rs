//! Everything you need to get started with Polars.
pub use std::sync::Arc;

pub use arrow::array::ArrayRef;
pub(crate) use arrow::array::*;
pub use arrow::datatypes::{ArrowSchema, Field as ArrowField};
pub use arrow::legacy::prelude::*;
pub(crate) use arrow::trusted_len::TrustedLen;
pub use polars_compute::rolling::{QuantileMethod, RollingFnParams, RollingVarParams};
pub use polars_utils::aliases::*;
pub use polars_utils::index::{ChunkId, IdxSize, NullableIdxSize};
pub use polars_utils::pl_str::PlSmallStr;
pub(crate) use polars_utils::total_ord::{TotalEq, TotalOrd};

pub(crate) use crate::chunked_array::ChunkLenIter;
pub use crate::chunked_array::ChunkedArray;
#[cfg(feature = "dtype-struct")]
pub use crate::chunked_array::StructChunked;
pub use crate::chunked_array::arithmetic::ArithmeticChunked;
pub use crate::chunked_array::builder::{
    BinaryChunkedBuilder, BooleanChunkedBuilder, ChunkedBuilder, ListBinaryChunkedBuilder,
    ListBooleanChunkedBuilder, ListBuilderTrait, ListPrimitiveChunkedBuilder,
    ListStringChunkedBuilder, NewChunkedArray, PrimitiveChunkedBuilder, StringChunkedBuilder,
};
pub use crate::chunked_array::collect::{ChunkedCollectInferIterExt, ChunkedCollectIterExt};
pub use crate::chunked_array::iterator::PolarsIterator;
#[cfg(feature = "dtype-categorical")]
pub use crate::chunked_array::logical::categorical::*;
#[cfg(feature = "ndarray")]
pub use crate::chunked_array::ndarray::IndexOrder;
#[cfg(feature = "object")]
pub use crate::chunked_array::object::PolarsObject;
pub use crate::chunked_array::ops::aggregate::*;
#[cfg(feature = "rolling_window")]
pub use crate::chunked_array::ops::rolling_window::RollingOptionsFixedWindow;
pub use crate::chunked_array::ops::*;
#[cfg(feature = "temporal")]
pub use crate::chunked_array::temporal::conversion::*;
pub use crate::datatypes::{ArrayCollectIterExt, *};
pub use crate::error::signals::try_raise_keyboard_interrupt;
pub use crate::error::{
    PolarsError, PolarsResult, polars_bail, polars_ensure, polars_err, polars_warn,
};
pub use crate::frame::column::{Column, IntoColumn};
pub use crate::frame::explode::UnpivotArgsIR;
#[cfg(feature = "algorithm_group_by")]
pub(crate) use crate::frame::group_by::aggregations::*;
#[cfg(feature = "algorithm_group_by")]
pub use crate::frame::group_by::*;
pub use crate::frame::{DataFrame, UniqueKeepStrategy};
pub use crate::hashing::VecHash;
pub use crate::named_from::{NamedFrom, NamedFromOwned};
pub use crate::scalar::Scalar;
pub use crate::schema::*;
#[cfg(feature = "checked_arithmetic")]
pub use crate::series::arithmetic::checked::NumOpsDispatchChecked;
pub use crate::series::arithmetic::{LhsNumOps, NumOpsDispatch};
pub use crate::series::implementations::null::NullChunked;
pub use crate::series::{IntoSeries, Series, SeriesTrait};
pub(crate) use crate::utils::CustomIterTools;
pub use crate::utils::IntoVec;
pub use crate::{datatypes, df, with_match_categorical_physical_type};

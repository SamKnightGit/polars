mod positioning;
mod unpivot;

use std::borrow::Cow;

use polars_core::frame::group_by::expr::PhysicalAggExpr;
use polars_core::prelude::*;
use polars_core::utils::_split_offsets;
use polars_core::{POOL, downcast_as_macro_arg_physical};
use polars_utils::format_pl_smallstr;
use rayon::prelude::*;
pub use unpivot::UnpivotDF;

const HASHMAP_INIT_SIZE: usize = 512;

#[derive(Clone)]
pub struct PivotAgg(pub Arc<dyn PhysicalAggExpr + Send + Sync>);

fn restore_logical_type(s: &Series, logical_type: &DataType) -> Series {
    // restore logical type
    match (logical_type, s.dtype()) {
        (DataType::Float32, DataType::UInt32) => {
            let ca = s.u32().unwrap();
            ca._reinterpret_float().into_series()
        },
        (DataType::Float64, DataType::UInt64) => {
            let ca = s.u64().unwrap();
            ca._reinterpret_float().into_series()
        },
        (DataType::Int32, DataType::UInt32) => {
            let ca = s.u32().unwrap();
            ca.reinterpret_signed()
        },
        (DataType::Int64, DataType::UInt64) => {
            let ca = s.u64().unwrap();
            ca.reinterpret_signed()
        },
        #[cfg(feature = "dtype-duration")]
        (DataType::Duration(_), DataType::UInt64) => {
            let ca = s.u64().unwrap();
            ca.reinterpret_signed().cast(logical_type).unwrap()
        },
        #[cfg(feature = "dtype-datetime")]
        (DataType::Datetime(_, _), DataType::UInt64) => {
            let ca = s.u64().unwrap();
            ca.reinterpret_signed().cast(logical_type).unwrap()
        },
        #[cfg(feature = "dtype-date")]
        (DataType::Date, DataType::UInt32) => {
            let ca = s.u32().unwrap();
            ca.reinterpret_signed().cast(logical_type).unwrap()
        },
        #[cfg(feature = "dtype-time")]
        (DataType::Time, DataType::UInt64) => {
            let ca = s.u64().unwrap();
            ca.reinterpret_signed().cast(logical_type).unwrap()
        },
        (dt, DataType::Null) => {
            let ca = Series::full_null(s.name().clone(), s.len(), dt);
            ca.into_series()
        },
        _ => unsafe { s.from_physical_unchecked(logical_type).unwrap() },
    }
}

/// Do a pivot operation based on the group key, a pivot column and an aggregation function on the values column.
///
/// # Note
/// Polars'/arrow memory is not ideal for transposing operations like pivots.
/// If you have a relatively large table, consider using a group_by over a pivot.
pub fn pivot<I0, I1, I2, S0, S1, S2>(
    pivot_df: &DataFrame,
    on: I0,
    index: Option<I1>,
    values: Option<I2>,
    sort_columns: bool,
    agg_fn: Option<PivotAgg>,
    separator: Option<&str>,
) -> PolarsResult<DataFrame>
where
    I0: IntoIterator<Item = S0>,
    I1: IntoIterator<Item = S1>,
    I2: IntoIterator<Item = S2>,
    S0: Into<PlSmallStr>,
    S1: Into<PlSmallStr>,
    S2: Into<PlSmallStr>,
{
    let on = on.into_iter().map(Into::into).collect::<Vec<_>>();
    let (index, values) = assign_remaining_columns(pivot_df, &on, index, values)?;
    pivot_impl(
        pivot_df,
        &on,
        &index,
        &values,
        agg_fn,
        sort_columns,
        false,
        separator,
    )
}

/// Do a pivot operation based on the group key, a pivot column and an aggregation function on the values column.
///
/// # Note
/// Polars'/arrow memory is not ideal for transposing operations like pivots.
/// If you have a relatively large table, consider using a group_by over a pivot.
pub fn pivot_stable<I0, I1, I2, S0, S1, S2>(
    pivot_df: &DataFrame,
    on: I0,
    index: Option<I1>,
    values: Option<I2>,
    sort_columns: bool,
    agg_fn: Option<PivotAgg>,
    separator: Option<&str>,
) -> PolarsResult<DataFrame>
where
    I0: IntoIterator<Item = S0>,
    I1: IntoIterator<Item = S1>,
    I2: IntoIterator<Item = S2>,
    S0: Into<PlSmallStr>,
    S1: Into<PlSmallStr>,
    S2: Into<PlSmallStr>,
{
    let on = on.into_iter().map(Into::into).collect::<Vec<_>>();
    let (index, values) = assign_remaining_columns(pivot_df, &on, index, values)?;
    pivot_impl(
        pivot_df,
        on.as_slice(),
        index.as_slice(),
        values.as_slice(),
        agg_fn,
        sort_columns,
        true,
        separator,
    )
}

/// Ensure both `index` and `values` are populated with `Vec<String>`.
///
/// - If `index` is None, assign columns not in `on` and `values` to it.
/// - If `values` is None, assign columns not in `on` and `index` to it.
/// - At least one of `index` and `values` must be non-null.
fn assign_remaining_columns<I1, I2, S1, S2>(
    df: &DataFrame,
    on: &[PlSmallStr],
    index: Option<I1>,
    values: Option<I2>,
) -> PolarsResult<(Vec<PlSmallStr>, Vec<PlSmallStr>)>
where
    I1: IntoIterator<Item = S1>,
    I2: IntoIterator<Item = S2>,
    S1: Into<PlSmallStr>,
    S2: Into<PlSmallStr>,
{
    match (index, values) {
        (Some(index), Some(values)) => {
            let index = index.into_iter().map(Into::into).collect();
            let values = values.into_iter().map(Into::into).collect();
            Ok((index, values))
        },
        (Some(index), None) => {
            let index: Vec<PlSmallStr> = index.into_iter().map(Into::into).collect();
            let values = df
                .get_column_names()
                .into_iter()
                .filter(|c| !(index.contains(c) | on.contains(c)))
                .cloned()
                .collect();
            Ok((index, values))
        },
        (None, Some(values)) => {
            let values: Vec<PlSmallStr> = values.into_iter().map(Into::into).collect();
            let index = df
                .get_column_names()
                .into_iter()
                .filter(|c| !(values.contains(c) | on.contains(c)))
                .cloned()
                .collect();
            Ok((index, values))
        },
        (None, None) => {
            polars_bail!(InvalidOperation: "`index` and `values` cannot both be None in `pivot` operation")
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn pivot_impl(
    pivot_df: &DataFrame,
    // keys of the first group_by operation
    on: &[PlSmallStr],
    // these columns will be aggregated in the nested group_by
    index: &[PlSmallStr],
    // these columns will be used for a nested group_by
    // the rows of this nested group_by will be pivoted as header column values
    values: &[PlSmallStr],
    // aggregation function
    agg_fn: Option<PivotAgg>,
    sort_columns: bool,
    stable: bool,
    // used as separator/delimiter in generated column names.
    separator: Option<&str>,
) -> PolarsResult<DataFrame> {
    polars_ensure!(!index.is_empty(), ComputeError: "index cannot be zero length");
    polars_ensure!(!on.is_empty(), ComputeError: "`on` cannot be zero length");
    if !stable {
        println!("unstable pivot not yet supported, using stable pivot");
    };
    if on.len() > 1 {
        let schema = Arc::new(pivot_df.schema());
        let binding = pivot_df.select_with_schema(on.iter().cloned(), &schema)?;
        let fields = binding.get_columns();
        let column = format_pl_smallstr!("{{\"{}\"}}", on.join("\",\""));
        if schema.contains(column.as_str()) {
            polars_bail!(ComputeError: "cannot use column name {column} that \
            already exists in the DataFrame. Please rename it prior to calling `pivot`.")
        }
        // @scalar-opt
        let columns_struct = StructChunked::from_columns(column.clone(), fields[0].len(), fields)
            .unwrap()
            .into_column();
        let mut binding = pivot_df.clone();
        let pivot_df = unsafe { binding.with_column_unchecked(columns_struct) };
        pivot_impl_single_column(
            pivot_df,
            index,
            &column,
            values,
            agg_fn,
            sort_columns,
            separator,
        )
    } else {
        pivot_impl_single_column(
            pivot_df,
            index,
            unsafe { on.get_unchecked(0) },
            values,
            agg_fn,
            sort_columns,
            separator,
        )
    }
}

fn pivot_impl_single_column(
    pivot_df: &DataFrame,
    index: &[PlSmallStr],
    column: &PlSmallStr,
    values: &[PlSmallStr],
    agg_fn: Option<PivotAgg>,
    sort_columns: bool,
    separator: Option<&str>,
) -> PolarsResult<DataFrame> {
    let sep = separator.unwrap_or("_");
    let mut final_cols = vec![];
    let mut count = 0;
    let out: PolarsResult<()> = POOL.install(|| {
        let mut group_by = index.to_vec();
        group_by.push(column.clone());

        let groups = pivot_df.group_by_stable(group_by)?.take_groups();

        let (col, row) = POOL.join(
            || positioning::compute_col_idx(pivot_df, column, &groups),
            || positioning::compute_row_idx(pivot_df, index, &groups, count),
        );
        let (col_locations, column_agg) = col?;
        let (row_locations, n_rows, mut row_index) = row?;

        for value_col_name in values {
            let value_col = pivot_df.column(value_col_name)?;

            // Aggregate the expression on a value column
            let value_agg = unsafe {
                match &agg_fn {
                    None => match value_col.len() > groups.len() {
                        true => polars_bail!(
                            ComputeError:
                            "found multiple elements in the same group, \
                            please specify an aggregation function"
                        ),
                        false => value_col.agg_first(&groups),
                    },
                    Some(agg_fn) => {
                        let expr = agg_fn.0.clone();
                        let name = expr.root_name()?.clone();
                        let mut value_col = value_col.clone();
                        value_col.rename(name);
                        let tmp_df = value_col.into_frame();
                        let mut aggregated =
                            Column::from(expr.evaluate_on_groups(&tmp_df, &groups)?);
                        aggregated.rename(value_col_name.clone());
                        aggregated
                    },
                }
            };

            // For any combination of 'index' and 'on' for which there is no entry in the df,
            // the default value is defined as the result of the agg_fn on the empty column.
            let default_val = {
                match &agg_fn {
                    None => AnyValue::Null,
                    Some(agg_fn) => {
                        let empty_col = Column::new_empty(PlSmallStr::EMPTY, value_col.dtype());
                        let empty_df = empty_col.clone().into_frame();
                        let empty_group = GroupsIdx::new_empty();
                        let groups_from_empty = GroupsType::from(empty_group).into_sliceable();
                        let expr = agg_fn.0.clone();
                        let agg_on_empty =
                            Column::from(expr.evaluate_on_groups(&empty_df, &groups_from_empty)?);
                        agg_on_empty.get(0).unwrap_or_default().into_static()
                    },
                }
            };

            let headers = column_agg.unique_stable()?.cast(&DataType::String)?;
            let mut headers = headers.str().unwrap().clone();
            if values.len() > 1 {
                headers = headers.apply_values(|v| Cow::from(format!("{value_col_name}{sep}{v}")))
            }

            let n_cols = headers.len();
            let value_agg_phys = value_agg.to_physical_repr();
            let logical_type = value_agg.dtype();

            debug_assert_eq!(row_locations.len(), col_locations.len());
            debug_assert_eq!(value_agg_phys.len(), row_locations.len());

            let mut cols = if value_agg_phys.dtype().is_primitive_numeric() {
                macro_rules! dispatch {
                    ($ca:expr) => {{
                        let default_val = default_val.extract();
                        positioning::position_aggregates_numeric(
                            n_rows,
                            n_cols,
                            &row_locations,
                            &col_locations,
                            $ca,
                            logical_type,
                            &headers,
                            default_val,
                        )
                    }};
                }
                downcast_as_macro_arg_physical!(value_agg_phys, dispatch)
            } else {
                positioning::position_aggregates(
                    n_rows,
                    n_cols,
                    &row_locations,
                    &col_locations,
                    value_agg_phys.as_materialized_series(),
                    logical_type,
                    &headers,
                    &default_val,
                )
            };

            if sort_columns {
                cols.sort_unstable_by(|a, b| a.name().partial_cmp(b.name()).unwrap());
            }

            let cols = if count == 0 {
                let mut final_cols = row_index.take().unwrap();
                final_cols.extend(cols);
                final_cols
            } else {
                cols
            };
            count += 1;
            final_cols.extend_from_slice(&cols);
        }
        Ok(())
    });
    out?;

    DataFrame::new(final_cols)
}

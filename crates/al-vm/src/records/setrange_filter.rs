//! The filter a Record's `SetRange` sets.

use super::*;

/// The range `SetRange` sets for the values after the field (`bounds`), converted to the field type `ty`; `None` when no value
/// is given (the field's filter is only removed). One value is the range from it to itself. Two Guids on a Media or MediaSet
/// field order them, which is not pinned (`al_builtins::filter::media_guid_order`): refused.
pub(crate) fn setrange_filter(ty: &RtType, bounds: &[Value]) -> Result<Option<FilterExpr>, Signal> {
    let Some(from) = bounds.first() else {
        return Ok(None);
    };
    let from = ops::convert(from.clone(), ty)?;
    let to = match bounds.get(1) {
        Some(v) => ops::convert(v.clone(), ty)?,
        None => from.clone(),
    };
    let range = FilterExpr::Range(Some(from), Some(to));
    if bounds.len() > 1 && al_builtins::filter::media_guid_order(ty, &range) {
        return Err(Signal::unsupported(
            "SetRange with two Guids on a Media or MediaSet field (the order of Guids is not pinned)",
        ));
    }
    Ok(Some(range))
}

impl Session {
    /// `Record.SetRange` on `rec` of table `t`: the field's filter in the current group is replaced by the range of the
    /// values after the field, or only removed when none is given.
    pub(crate) fn set_range(&mut self, rec: &Rec, t: &TableDef, args: &[Value]) -> Result<Value, Signal> {
        let pos = arg_pos(args, 0)?;
        let ty = t.fields[pos].ty.clone();
        let or_flowfield = self.is_flowfield(t, pos);
        self.blob_setrange_check(&ty, &args[1..])?;
        let mut r = rec.borrow_mut();
        r.position = None;
        let group = r.filter_group;
        r.filters.retain(|f| !(f.field == pos && f.group == group));
        r.decimal_cut_filters.retain(|f| !(f.field == pos && f.group == group));
        // A FlowField filter in the OR group is dropped: GetFilters does not show it and no row is filtered by it
        // (record/PWFilterGroupOr.al:OrGroup_FlowFieldFilter_BQ, `getFilters`, `flowFieldOnly`, `nameOrFlowField`).
        if group == al_builtins::filter::OR_GROUP && or_flowfield {
            return Ok(Value::Null);
        }
        if let Some(range) = setrange_filter(&ty, &args[1..])? {
            let filter = FieldFilter::new(pos, range, group);
            self.mark_decimal_cut_filter(rec, &mut r, t, (RecordOp::SetRange, &filter), None, args);
            r.filters.push(filter);
        }
        Ok(Value::Null)
    }
}

/*!
Traits, methods and types to handle joining or merging two
[DataView](../view/struct.DataView.html)s. Joining `DataView`s involves finding the rows in each
`DataView` which satisfy a specific join predicate (much like a `JOIN` in a SQL database). Merging
refers to combining fields of two `DataView` objects with the same number of rows into a single
`DataView`.
*/
use std::cmp::Ordering;
use std::marker::PhantomData;
use std::ops::Add;

use access::*;
use cons::*;
use error::*;
use field::Value;
use label::*;
use select::*;
use store::{AssocStorage, DataStore, IntoView, PushBackClonedFromValueIter};
use view::*;

/// A trait for applying a frame index offset `O`.
pub trait Offset<O> {
    /// Result after applying offset `O`.
    type Output;
}
impl<O, U> Offset<O> for U
where
    U: Add<O>,
{
    type Output = <U as Add<O>>::Output;
}

/// Trait for updating the frame index of a [FrameLookupCons](../view/type.FrameLookupCons.html)
/// by a specified [Offset](trait.Offset.html).
pub trait UpdateFrameIndexMarker<FrameIndexOffset> {
    /// `FrameLookupCons` after updating frame index by `FrameIndexOffset`.
    type Output;
}
impl<FrameIndexOffset> UpdateFrameIndexMarker<FrameIndexOffset> for Nil {
    type Output = Nil;
}
impl<RLabel, RFrameIndex, RFrameLabel, RTail, FrameIndexOffset>
    UpdateFrameIndexMarker<FrameIndexOffset>
    for FrameLookupCons<RLabel, RFrameIndex, RFrameLabel, RTail>
where
    RFrameIndex: Offset<FrameIndexOffset>,
    RTail: UpdateFrameIndexMarker<FrameIndexOffset>,
{
    type Output = FrameLookupCons<
        RLabel,
        <RFrameIndex as Offset<FrameIndexOffset>>::Output,
        RFrameLabel,
        <RTail as UpdateFrameIndexMarker<FrameIndexOffset>>::Output,
    >;
}

/// Trait for updating the frame index of a [ViewFrameCons](../view/type.ViewFrameCons.html)
/// by a specified [Offset](trait.Offset.html).
pub trait UpdateFrameIndex<FrameIndexOffset> {
    /// `ViewFrameCons` after updating frame index by `FrameIndexOffset`.
    type Output;

    /// Update `ViewFrameCons` with frame index offset.
    fn update_frame_label(self) -> Self::Output;
}
impl<FrameIndexOffset> UpdateFrameIndex<FrameIndexOffset> for Nil {
    type Output = Nil;

    fn update_frame_label(self) -> Nil {
        Nil
    }
}

impl<RFrameIndex, RFrameFields, RTail, FrameIndexOffset> UpdateFrameIndex<FrameIndexOffset>
    for ViewFrameCons<RFrameIndex, RFrameFields, RTail>
where
    RFrameIndex: Offset<FrameIndexOffset>,
    RFrameFields: AssocStorage,
    RTail: UpdateFrameIndex<FrameIndexOffset>,
{
    type Output = ViewFrameCons<
        <RFrameIndex as Offset<FrameIndexOffset>>::Output,
        RFrameFields,
        <RTail as UpdateFrameIndex<FrameIndexOffset>>::Output,
    >;

    fn update_frame_label(self) -> Self::Output {
        LVCons {
            head: Labeled::from(self.head.value),
            tail: self.tail.update_frame_label(),
        }
    }
}

/// Trait for merging another [DataView](../view/struct.DataView.html) with `self`. `RLabels` and
/// `RFrames` are the type parameters of the `DataView` being merged onto `self`.
pub trait Merge<RLabels, RFrames> {
    /// Resulting `Labels` type parameter of merged `DataView` object.
    type OutLabels;
    /// Resulting `Frames` type parameterof merged `DataView` object.
    type OutFrames;

    /// Merge this object with `right`, producing a new `DataView` with `OutLabels` and `OutFrames`
    /// type parameters.
    fn merge(
        &self,
        right: &DataView<RLabels, RFrames>,
    ) -> DataView<Self::OutLabels, Self::OutFrames>;
}
impl<LLabels, LFrames, RLabels, RFrames> Merge<RLabels, RFrames> for DataView<LLabels, LFrames>
where
    LFrames: Len,
    RLabels: UpdateFrameIndexMarker<<LFrames as Len>::Len>,
    LLabels: Append<<RLabels as UpdateFrameIndexMarker<<LFrames as Len>::Len>>::Output>,
    RFrames: Clone + UpdateFrameIndex<<LFrames as Len>::Len>,
    LFrames: Append<<RFrames as UpdateFrameIndex<<LFrames as Len>::Len>>::Output> + Clone,
{
    type OutLabels = <LLabels as Append<
        <RLabels as UpdateFrameIndexMarker<<LFrames as Len>::Len>>::Output,
    >>::Appended;
    type OutFrames =
        <LFrames as Append<<RFrames as UpdateFrameIndex<<LFrames as Len>::Len>>::Output>>::Appended;

    fn merge(
        &self,
        right: &DataView<RLabels, RFrames>,
    ) -> DataView<Self::OutLabels, Self::OutFrames> {
        let out_frames = self
            .frames
            .clone()
            .append(right.frames.clone().update_frame_label());

        DataView {
            _labels: PhantomData,
            frames: out_frames,
        }
    }
}

/// Marker struct describing a join. `LLabel` is the label of the left-hand side, `RLabel` is the
/// label of the right-hand side, and `Predicate` represents the type of join predicate (equal join,
/// greater-than join, less-than join, etc.).
pub struct Join<LLabel, RLabel, Predicate> {
    _marker: PhantomData<(LLabel, RLabel, Predicate)>,
}

/// A trait for describing the course of action in a sort-merge join. This trait differentiates
/// the actions that are taken during a sort-merge join based on the implementing type.
pub trait Predicate {
    /// Returns `true` if predicate is an 'equality' predicate (`==`, `<=`, `=>`)
    fn is_equality_pred() -> bool;
    /// Returns `true` if predicate is a 'greater than' predicate (`>=`, `>`)
    fn is_greater_than_pred() -> bool;
    /// Returns `true` if predciate is a 'less than' predicate (`<=`, `<`)
    fn is_less_than_pred() -> bool;
    /// Applies this predicate to the two values and returns the desired action.
    fn apply<T>(left: Value<&T>, right: Value<&T>) -> PredAction
    where
        T: PartialEq + Ord;
    /// Advances indices appropriately as required for this predicate type.
    fn advance(left_idx: &mut usize, right_idx: &mut usize, left_end: usize, right_end: usize);
}

/// Predicate for equality joins (left == right).
pub struct Equal;
impl Predicate for Equal {
    fn is_equality_pred() -> bool {
        true
    }
    fn is_greater_than_pred() -> bool {
        false
    }
    fn is_less_than_pred() -> bool {
        false
    }
    fn apply<T>(left: Value<&T>, right: Value<&T>) -> PredAction
    where
        T: PartialEq + Ord,
    {
        match left.cmp(&right) {
            Ordering::Less => PredAction::Advance {
                left: true,
                right: false,
            },
            Ordering::Equal => PredAction::Add,
            Ordering::Greater => PredAction::Advance {
                left: false,
                right: true,
            },
        }
    }
    fn advance(left_idx: &mut usize, right_idx: &mut usize, left_end: usize, right_end: usize) {
        *left_idx = left_end;
        *right_idx = right_end;
    }
}

/// Predicate for less-than joins (left < right).
pub struct LessThan;
impl Predicate for LessThan {
    fn is_equality_pred() -> bool {
        false
    }
    fn is_greater_than_pred() -> bool {
        false
    }
    fn is_less_than_pred() -> bool {
        true
    }
    fn apply<T>(left: Value<&T>, right: Value<&T>) -> PredAction
    where
        T: PartialEq + Ord,
    {
        match left.cmp(&right) {
            Ordering::Less => PredAction::Add,
            _ => PredAction::Advance {
                left: false,
                right: true,
            },
        }
    }
    fn advance(left_idx: &mut usize, _right_idx: &mut usize, _left_end: usize, _right_end: usize) {
        *left_idx += 1;
    }
}

/// Predicate for less-than-equal joins (left <= right).
pub struct LessThanEqual;
impl Predicate for LessThanEqual {
    fn is_equality_pred() -> bool {
        true
    }
    fn is_greater_than_pred() -> bool {
        false
    }
    fn is_less_than_pred() -> bool {
        true
    }
    fn apply<T>(left: Value<&T>, right: Value<&T>) -> PredAction
    where
        T: PartialEq + Ord,
    {
        match left.cmp(&right) {
            Ordering::Greater => PredAction::Advance {
                left: false,
                right: true,
            },
            _ => PredAction::Add,
        }
    }
    fn advance(left_idx: &mut usize, _right_idx: &mut usize, left_end: usize, _right_end: usize) {
        *left_idx = left_end;
    }
}

/// Predicate for greater-than joins (left > right).
pub struct GreaterThan;
impl Predicate for GreaterThan {
    fn is_equality_pred() -> bool {
        false
    }
    fn is_greater_than_pred() -> bool {
        true
    }
    fn is_less_than_pred() -> bool {
        false
    }
    fn apply<T>(left: Value<&T>, right: Value<&T>) -> PredAction
    where
        T: PartialEq + Ord,
    {
        match left.cmp(&right) {
            Ordering::Greater => PredAction::Add,
            _ => PredAction::Advance {
                left: true,
                right: false,
            },
        }
    }
    fn advance(_left_idx: &mut usize, right_idx: &mut usize, _left_end: usize, _right_end: usize) {
        *right_idx += 1;
    }
}

/// Predicate for greater-than-equal joins (left >= right).
pub struct GreaterThanEqual;
impl Predicate for GreaterThanEqual {
    fn is_equality_pred() -> bool {
        true
    }
    fn is_greater_than_pred() -> bool {
        true
    }
    fn is_less_than_pred() -> bool {
        false
    }
    fn apply<T>(left: Value<&T>, right: Value<&T>) -> PredAction
    where
        T: PartialEq + Ord,
    {
        match left.cmp(&right) {
            Ordering::Less => PredAction::Advance {
                left: true,
                right: false,
            },
            _ => PredAction::Add,
        }
    }
    fn advance(_left_idx: &mut usize, right_idx: &mut usize, _left_end: usize, right_end: usize) {
        *right_idx = right_end;
    }
}

/// The action to take in the sort-merge join algorithm as a result of the selected predicate.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PredAction {
    /// Predicate matches, add current indices to join result.
    Add,
    /// Advance indices (either left, right, or both).
    Advance {
        /// Advance left index.
        left: bool,
        /// Advance right index.
        right: bool,
    },
}

/// A trait for merging a [DataView](../view/struct.DataView.html) with the current object using
/// specified `Join`. `RLabels` and `RFrames` are the `Labels` and `Frames` type parameters for the
/// `DataView` to merge.
pub trait SortMergeJoin<RLabels, RFrames, Join> {
    /// Resultant data structure after join.
    type Output;

    /// Join this object with a `DataView`, using the join details specified with `Join`.
    fn join(&self, right: &DataView<RLabels, RFrames>) -> Self::Output;
}
impl<LLabels, LFrames, RLabels, RFrames, LLabel, RLabel, Pred>
    SortMergeJoin<RLabels, RFrames, Join<LLabel, RLabel, Pred>> for DataView<LLabels, LFrames>
where
    LFrames: JoinIntoStore<LLabels, DataStore<Nil>>,
    RFrames: JoinIntoStore<RLabels, <LFrames as JoinIntoStore<LLabels, DataStore<Nil>>>::Output>,
    <RFrames as JoinIntoStore<
        RLabels,
        <LFrames as JoinIntoStore<LLabels, DataStore<Nil>>>::Output,
    >>::Output: IntoView,
    Self: SelectFieldByLabel<LLabel>,
    <Self as SelectFieldByLabel<LLabel>>::Output: SortOrder,
    VFieldTypeOf<Self, LLabel>: Ord + PartialEq,
    DataView<RLabels, RFrames>: SelectFieldByLabel<RLabel>,
    <DataView<RLabels, RFrames> as SelectFieldByLabel<RLabel>>::Output: SortOrder,
    VFieldOf<DataView<RLabels, RFrames>, RLabel>: DataIndex<DType = VFieldTypeOf<Self, LLabel>>,
    Pred: Predicate,
{
    type Output = <<RFrames as JoinIntoStore<
        RLabels,
        <LFrames as JoinIntoStore<LLabels, DataStore<Nil>>>::Output,
    >>::Output as IntoView>::Output;

    fn join(&self, right: &DataView<RLabels, RFrames>) -> Self::Output {
        let left = self;
        //TODO: return empty dataview if left or right is empty

        let merge_indices =
            merge_indices::<Pred, _, _>(&left.field::<LLabel>(), &right.field::<RLabel>());

        let store = DataStore::<Nil>::empty();

        let store = left
            .frames
            .join_into_store(store, &merge_indices.0)
            .unwrap();
        let store = right
            .frames
            .join_into_store(store, &merge_indices.1)
            .unwrap();
        store.into_view()
    }
}

fn merge_indices<Pred, T, U>(left_key_data: &T, right_key_data: &U) -> (Vec<usize>, Vec<usize>)
where
    Pred: Predicate,
    T: DataIndex + SortOrder,
    U: DataIndex<DType = <T as DataIndex>::DType> + SortOrder,
    <T as DataIndex>::DType: PartialEq + Ord,
{
    let left_order = left_key_data.sort_order();
    let right_order = right_key_data.sort_order();

    debug_assert!(!left_order.is_empty() && !right_order.is_empty());
    // NOTE: actual_idx = perm[sorted_idx]
    // NOTE: value = key_data.get(actual_idx).unwrap();

    let lval = |sorted_idx| left_key_data.get_datum(left_order[sorted_idx]).unwrap();
    let rval = |sorted_idx| right_key_data.get_datum(right_order[sorted_idx]).unwrap();

    // we know left_order and right_order both are non-empty, so there is at least one value
    let (mut left_idx, mut right_idx) = (0, 0);
    let mut left_merge_indices = vec![];
    let mut right_merge_indices = vec![];
    while left_idx < left_order.len() && right_idx < right_order.len() {
        let left_val = lval(left_idx);
        let right_val = rval(right_idx);
        let pred_results = Pred::apply(left_val, right_val);
        match pred_results {
            PredAction::Add => {
                // figure out subsets
                let mut left_subset = vec![left_idx];
                let mut right_subset = vec![right_idx];
                let (mut left_idx_end, mut right_idx_end);
                if Pred::is_equality_pred() {
                    // for equality predicates, add all records with same value
                    left_idx_end = left_idx + 1;
                    while left_idx_end < left_order.len() && left_val == lval(left_idx_end) {
                        left_subset.push(left_idx_end);
                        left_idx_end += 1;
                    }
                    right_idx_end = right_idx + 1;
                    while right_idx_end < right_order.len() && right_val == rval(right_idx_end) {
                        right_subset.push(right_idx_end);
                        right_idx_end += 1;
                    }
                } else {
                    left_idx_end = left_idx + 1;
                    right_idx_end = right_idx + 1;
                }
                let (left_eq_end, right_eq_end) = (left_idx_end, right_idx_end);
                if Pred::is_greater_than_pred() {
                    // for greater-than predicates, we can add the rest of the left values
                    while left_idx_end < left_order.len() {
                        left_subset.push(left_idx_end);
                        left_idx_end += 1;
                    }
                }
                if Pred::is_less_than_pred() {
                    // for less-than predicates, we can add the rest of the right values
                    while right_idx_end < right_order.len() {
                        right_subset.push(right_idx_end);
                        right_idx_end += 1;
                    }
                }
                // add cross product of subsets to merge indices
                for lidx in &left_subset {
                    // NAs shouldn't match a predicate, only add if value exists
                    if lval(*lidx).exists() {
                        for ridx in &right_subset {
                            if rval(*ridx).exists() {
                                left_merge_indices.push(left_order[*lidx]);
                                right_merge_indices.push(right_order[*ridx]);
                            }
                        }
                    }
                }
                // advance as needed
                Pred::advance(&mut left_idx, &mut right_idx, left_eq_end, right_eq_end);
            }
            PredAction::Advance { left, right } => {
                if left {
                    left_idx += 1;
                }
                if right {
                    right_idx += 1;
                }
            }
        }
    }
    (left_merge_indices, right_merge_indices)
}

/// A trait for augmenting type `Store` (a [DataStore](../store/struct.DataStore.html)) with
/// fields from this [ViewFrameCons](../view/type.ViewFrameCons.html) as labeled by `Labels`.
pub trait JoinIntoStore<Labels, Store> {
    /// The output type after augmenting `Store`.
    type Output;

    /// Augments `store` with data from `self` (as specified with `Labels`), using the provided
    /// permutation indices.
    fn join_into_store(&self, store: Store, permutation: &[usize]) -> Result<Self::Output>;
}
impl<Frames, Store> JoinIntoStore<Nil, Store> for Frames {
    type Output = Store;
    fn join_into_store(&self, store: Store, _permutation: &[usize]) -> Result<Store> {
        Ok(store)
    }
}
impl<Label, FrameIndex, FrameLabel, Tail, Frames, Store>
    JoinIntoStore<FrameLookupCons<Label, FrameIndex, FrameLabel, Tail>, Store> for Frames
where
    Frames: LookupValuedElemByLabel<FrameIndex>,
    FrameByFrameIndexOf<Frames, FrameIndex>: SelectFieldByLabel<FrameLabel>,
    Store: PushBackClonedFromValueIter<
        Label,
        FieldTypeFromFrameDetailsOf<Frames, FrameIndex, FrameLabel>,
    >,
    Frames: JoinIntoStore<
        Tail,
        DataStore<
            <Store as PushBackClonedFromValueIter<
                Label,
                FieldTypeFromFrameDetailsOf<Frames, FrameIndex, FrameLabel>,
            >>::OutputFields,
        >,
    >,
{
    type Output = <Frames as JoinIntoStore<
        Tail,
        DataStore<
            <Store as PushBackClonedFromValueIter<
                Label,
                FieldTypeFromFrameDetailsOf<Frames, FrameIndex, FrameLabel>,
            >>::OutputFields,
        >,
    >>::Output;

    fn join_into_store(&self, store: Store, permutation: &[usize]) -> Result<Self::Output> {
        let store = store.push_back_cloned_from_value_iter(
            SelectFieldByLabel::<FrameLabel>::select_field(
                LookupValuedElemByLabel::<FrameIndex>::elem(self).value_ref(),
            )
            .permute(permutation)?,
        );
        let store = JoinIntoStore::<Tail, _>::join_into_store(self, store, permutation)?;
        Ok(store)
    }
}

#[cfg(feature = "test-utils")]
#[cfg(test)]
mod tests {
    use super::*;
    use field::FieldData;
    use test_utils::*;

    #[test]
    fn inner_equi_join() {
        let dv_emp = sample_emp_table().into_view();
        let dv_dept = sample_dept_table().into_view();
        println!("{}", dv_emp);
        println!("{}", dv_dept);

        let joined_dv =
            dv_emp.join::<Join<emp_table::DeptId, dept_table::DeptId, Equal>, _, _>(&dv_dept);
        println!("{}", joined_dv);
        assert_eq!(joined_dv.nrows(), 7);
        assert_eq!(joined_dv.nfields(), 5);
        assert_eq!(
            joined_dv.field::<emp_table::EmpId>().to_vec(),
            vec![0u64, 5, 6, 2, 8, 9, 10]
        );
        assert_eq!(
            joined_dv.field::<emp_table::DeptId>().to_vec(),
            vec![1u64, 1, 1, 2, 3, 4, 4]
        );
        assert_eq!(
            joined_dv.field::<emp_table::EmpName>().to_vec(),
            vec!["Sally", "Bob", "Cara", "Jamie", "Louis", "Louise", "Ann"]
        );
        assert_eq!(
            joined_dv.field::<dept_table::DeptName>().to_vec(),
            vec![
                "Marketing",
                "Marketing",
                "Marketing",
                "Sales",
                "Manufacturing",
                "R&D",
                "R&D"
            ]
        );
    }

    #[test]
    fn inner_equi_join_missing_dept_id() {
        // dept id missing from dept table, should remove the entire marketing department from join
        let dv_emp = sample_emp_table().into_view();
        let dv_dept = dept_table_from_field(
            FieldData::from_field_vec(vec![
                Value::Na,
                Value::Exists(2),
                Value::Exists(3),
                Value::Exists(4),
            ]),
            FieldData::from_field_vec(vec![
                Value::Exists("Marketing".into()),
                Value::Exists("Sales".into()),
                Value::Exists("Manufacturing".into()),
                Value::Exists("R&D".into()),
            ]),
        )
        .into_view();

        println!("{}", dv_emp);
        println!("{}", dv_dept);

        let joined_dv =
            dv_emp.join::<Join<emp_table::DeptId, dept_table::DeptId, Equal>, _, _>(&dv_dept);
        println!("{}", joined_dv);

        assert_eq!(joined_dv.nrows(), 4);
        assert_eq!(joined_dv.nfields(), 5);
        assert_eq!(
            joined_dv.field::<emp_table::EmpId>().to_vec(),
            vec![2u64, 8, 9, 10]
        );
        assert_eq!(
            joined_dv.field::<emp_table::DeptId>().to_vec(),
            vec![2u64, 3, 4, 4]
        );
        assert_eq!(
            joined_dv.field::<emp_table::EmpName>().to_vec(),
            vec!["Jamie", "Louis", "Louise", "Ann"]
        );
        assert_eq!(
            joined_dv.field::<dept_table::DeptName>().to_vec(),
            vec!["Sales", "Manufacturing", "R&D", "R&D"]
        );

        // dept id missing from emp table, should remove single employee from join
        let ds_emp: emp_table::Store = emp_table_from_field!(
            FieldData::from_field_vec(vec![
                Value::Exists(0),
                Value::Exists(2),
                Value::Exists(5),
                Value::Exists(6),
                Value::Exists(8),
                Value::Exists(9),
                Value::Exists(10),
            ]),
            FieldData::from_field_vec(vec![
                Value::Exists(1),
                Value::Exists(2),
                Value::Na, // Bob's department isn't specified
                Value::Exists(1),
                Value::Exists(3),
                Value::Exists(4),
                Value::Exists(4),
            ]),
            FieldData::from_field_vec(vec![
                Value::Exists("Sally".into()),
                Value::Exists("Jamie".into()),
                Value::Exists("Bob".into()),
                Value::Exists("Cara".into()),
                Value::Exists("Louis".into()),
                Value::Exists("Louise".into()),
                Value::Exists("Ann".into()),
            ])
        );
        let dv_emp = ds_emp.into_view();
        let dv_dept = sample_dept_table().into_view();
        println!("{}", dv_emp);
        println!("{}", dv_dept);
        let joined_dv =
            dv_emp.join::<Join<emp_table::DeptId, dept_table::DeptId, Equal>, _, _>(&dv_dept);
        println!("{}", joined_dv);
        assert_eq!(joined_dv.nrows(), 6);
        assert_eq!(joined_dv.nfields(), 5);
        assert_eq!(
            joined_dv.field::<emp_table::EmpId>().to_vec(),
            vec![0u64, 6, 2, 8, 9, 10]
        );
        assert_eq!(
            joined_dv.field::<emp_table::DeptId>().to_vec(),
            vec![1u64, 1, 2, 3, 4, 4]
        );
        assert_eq!(
            joined_dv.field::<emp_table::EmpName>().to_vec(),
            vec!["Sally", "Cara", "Jamie", "Louis", "Louise", "Ann"]
        );
        assert_eq!(
            joined_dv.field::<dept_table::DeptName>().to_vec(),
            vec![
                "Marketing",
                "Marketing",
                "Sales",
                "Manufacturing",
                "R&D",
                "R&D"
            ]
        );
    }

    #[test]
    fn filter_inner_equi_join() {
        // should have same results as first test in inner_equi_join_missing_dept_id
        let dv_emp = sample_emp_table().into_view();
        let mut dv_dept = sample_dept_table().into_view();
        println!("{}", dv_emp);
        println!("{}", dv_dept);

        dv_dept.filter::<dept_table::DeptId, _>(|val: Value<&u64>| val != valref![1u64]);
        println!("{}", dv_dept);
        let joined_dv =
            dv_emp.join::<Join<emp_table::DeptId, dept_table::DeptId, Equal>, _, _>(&dv_dept);
        println!("{}", joined_dv);
        assert_eq!(joined_dv.nrows(), 4);
        assert_eq!(joined_dv.nfields(), 5);
        assert_eq!(
            joined_dv.field::<emp_table::EmpId>().to_vec(),
            vec![2u64, 8, 9, 10]
        );
        assert_eq!(
            joined_dv.field::<emp_table::DeptId>().to_vec(),
            vec![2u64, 3, 4, 4]
        );
        assert_eq!(
            joined_dv.field::<emp_table::EmpName>().to_vec(),
            vec!["Jamie", "Louis", "Louise", "Ann"]
        );
        assert_eq!(
            joined_dv.field::<dept_table::DeptName>().to_vec(),
            vec!["Sales", "Manufacturing", "R&D", "R&D"]
        );
    }

    namespace![
        @continue(typenum::Add1<::test_utils::dept_table::Table>)
        table dept_rename {
            RDeptId: u64
        }
    ];

    #[test]
    fn inner_nonequi_join() {
        // greater than
        let dv_emp = sample_emp_table().into_view();
        let dv_dept = dept_table(vec![1, 2], vec!["Marketing", "Sales"]).into_view();
        println!("{}", dv_emp);
        println!("{}", dv_dept);

        let dv_dept = dv_dept.relabel::<dept_table::DeptId, dept_rename::RDeptId>();
        // also test relabeling
        let joined_dv = dv_emp
            .join::<Join<emp_table::DeptId, dept_rename::RDeptId, GreaterThan>, _, _>(&dv_dept);
        println!("{}", joined_dv);
        assert_eq!(joined_dv.nrows(), 7);
        assert_eq!(joined_dv.nfields(), 5);
        for value in joined_dv.field::<emp_table::DeptId>().iter() {
            assert![*value.unwrap() >= 2];
        }

        // greater than equal
        let dv_emp = sample_emp_table().into_view();
        let dv_dept = dept_table(vec![2], vec!["Sales"]).into_view();
        let joined_dv = dv_emp
            .join::<Join<emp_table::DeptId, dept_table::DeptId, GreaterThanEqual>, _, _>(&dv_dept);
        println!("{}", joined_dv);
        assert_eq!(joined_dv.nrows(), 4);
        assert_eq!(joined_dv.nfields(), 5);
        for value in joined_dv.field::<emp_table::DeptId>().iter() {
            assert![*value.unwrap() >= 2];
        }

        // less than
        let dv_emp = sample_emp_table().into_view();
        let dv_dept = dept_table(vec![2], vec!["Sales"]).into_view();
        let joined_dv =
            dv_emp.join::<Join<emp_table::DeptId, dept_table::DeptId, LessThan>, _, _>(&dv_dept);
        println!("{}", joined_dv);
        assert_eq!(joined_dv.nrows(), 3);
        assert_eq!(joined_dv.nfields(), 5);
        for value in joined_dv.field::<emp_table::DeptId>().iter() {
            assert_eq![*value.unwrap(), 1];
        }

        // less than equal
        let dv_emp = sample_emp_table().into_view();
        let dv_dept = dept_table(vec![2], vec!["Sales"]).into_view();
        let joined_dv = dv_emp
            .join::<Join<emp_table::DeptId, dept_table::DeptId, LessThanEqual>, _, _>(&dv_dept);
        println!("{}", joined_dv);
        assert_eq!(joined_dv.nrows(), 4);
        assert_eq!(joined_dv.nfields(), 5);
        for value in joined_dv.field::<emp_table::DeptId>().iter() {
            assert![*value.unwrap() <= 2];
        }
    }
}

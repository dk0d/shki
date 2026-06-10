use indexmap::IndexMap;

use crate::models::iden::Iden;
use crate::schema::{Constraint, Table};

/// port of https://github.com/pgplex/pgschema/blob/main/internal/diff/topological.go
pub(super) fn sort_created_tables(tables: Vec<Table>) -> Vec<Table> {
    if tables.len() <= 1 {
        // already sorted
        return tables;
    }

    let mut table_map: IndexMap<Iden, Table> = IndexMap::new();

    let mut insertion_order = Vec::new();

    for table in tables {
        let id = table.id();
        insertion_order.push(id.clone());
        table_map.insert(id, table);
    }

    let mut incoming: IndexMap<Iden, usize> = IndexMap::new();

    let mut outgoing: IndexMap<Iden, Vec<Iden>> = IndexMap::new();

    // Init
    for id in table_map.keys() {
        incoming.insert(id.clone(), 0);
        outgoing.insert(id.clone(), Vec::new());
    }

    // Build edges: if table A has FK to table B, add edge B->A
    for (table_id, table) in &table_map {
        for constraint in &table.constraints {
            let Constraint::ForeignKey(fk) = constraint else {
                continue;
            };
            let referenced = Iden::new(
                fk.references.name.clone(),
                fk.references
                    .schema
                    .clone()
                    .or_else(|| table.schema.clone()),
            );
            if table_map.contains_key(&referenced) && &referenced != table_id {
                outgoing
                    .entry(referenced.clone())
                    .or_default()
                    .push(table_id.clone());
                *incoming.entry(table_id.clone()).or_insert(0) += 1;
            }
        }
    }

    // ---
    // Kahn's algo with deterministic cycle breaking
    // ---

    // init queue with nodes that have no incoming edges
    let mut queue = incoming
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(id.clone()))
        .collect::<Vec<_>>();
    queue.sort_by_key(stable_id_key);

    let mut result = Vec::new();
    let mut processed: IndexMap<Iden, bool> = IndexMap::new();

    while result.len() < table_map.len() {
        // Cycle detected: pick the next unprocessed table using original insertion order
        //
        // CYCLE BREAKING STRATEGY:
        // Setting incoming[iden] = 0 effectively declares "this table has no remaining dependencies"
        // for the purpose of breaking the cycle. This is safe because:
        //
        // 1. The 'processed' map prevents any table from being added to the result twice, even if
        //    its inDegree becomes zero or negative multiple times (see line 92 check).
        //
        // 2. For circular foreign key dependencies (e.g., A↔B), the table creation order doesn't
        //    matter because shki follows PostgreSQL's pattern of creating tables first and
        //    adding foreign key constraints afterwards via ALTER TABLE statements.
        //
        // 3. Using insertion order (alphabetical by schema.name) ensures deterministic output
        //    when multiple valid orderings exist.
        //
        // This approach aligns with PostgreSQL's pg_dump, which breaks dependency cycles by
        // separating table creation from constraint creation.
        if queue.is_empty() {
            if let Some(next) = insertion_order
                .iter()
                .find(|id| !processed.contains_key(*id))
                .cloned()
            {
                incoming.insert(next.clone(), 0);
                queue.push(next);
            } else {
                break;
            }
        }

        let current = queue.remove(0);
        if processed.contains_key(&current) {
            continue;
        }
        processed.insert(current.clone(), true);
        result.push(current.clone());

        let mut neighbors = outgoing.get(&current).cloned().unwrap_or_default();
        neighbors.sort_by_key(stable_id_key);
        for neighbor in neighbors {
            if processed.contains_key(&neighbor) {
                continue;
            }
            if let Some(count) = incoming.get_mut(&neighbor) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    queue.push(neighbor);
                    queue.sort_by_key(stable_id_key);
                }
            }
        }
    }

    result
        .into_iter()
        .filter_map(|id| table_map.shift_remove(&id))
        .collect()
}

fn stable_id_key(id: &Iden) -> String {
    id.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Column, DataType, ForeignKeyConstraint};

    #[test]
    fn sorts_referenced_tables_before_referencing_tables() {
        let mut child = Table::in_schema("child", "public");
        child.column(Column::new("parent_id", DataType::Integer));
        child.constraint(Constraint::ForeignKey(
            ForeignKeyConstraint::new(
                vec!["parent_id"],
                Iden::new("parent", Some("public".to_string())),
                vec!["id"],
            )
            .named("child_parent_fkey"),
        ));

        let mut parent = Table::in_schema("parent", "public");
        parent.column(Column::new("id", DataType::Integer));

        let sorted = sort_created_tables(vec![child, parent]);

        assert_eq!(sorted[0].name, "parent");
        assert_eq!(sorted[1].name, "child");
    }
}

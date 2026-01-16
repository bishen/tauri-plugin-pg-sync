//! 本地数据库单元测试
//!
//! 测试数据完整性、事务支持、跨平台兼容性

#[cfg(test)]
mod local_db_tests {
    use crate::db::LocalDb;
    use serde_json::json;

    fn setup_test_db() -> LocalDb {
        LocalDb::in_memory().expect("Failed to create in-memory database")
    }

    #[test]
    fn test_node_id_persistence() {
        let db = setup_test_db();

        // 第一次获取应该创建新的 node_id
        let node_id1 = db.get_or_create_node_id().expect("Failed to get node_id");
        assert!(!node_id1.is_empty());

        // 第二次获取应该返回相同的 node_id
        let node_id2 = db.get_or_create_node_id().expect("Failed to get node_id");
        assert_eq!(node_id1, node_id2);
    }

    #[test]
    fn test_ensure_table_creates_sync_fields() {
        let db = setup_test_db();

        let columns = vec![
            ("name".to_string(), "TEXT".to_string()),
            ("age".to_string(), "INTEGER".to_string()),
        ];

        db.ensure_table("users", &columns)
            .expect("Failed to create table");

        // 验证表结构包含同步元字段
        let schema = db.get_table_schema("users").expect("Failed to get schema");
        let field_names: Vec<&str> = schema.iter().map(|c| c.name.as_str()).collect();

        assert!(field_names.contains(&"id"));
        assert!(field_names.contains(&"name"));
        assert!(field_names.contains(&"age"));
        assert!(field_names.contains(&"_hlc"));
        assert!(field_names.contains(&"_node_id"));
        assert!(field_names.contains(&"_version"));
        assert!(field_names.contains(&"_deleted"));
        assert!(field_names.contains(&"_synced"));
    }

    #[test]
    fn test_insert_and_find_by_id() {
        let db = setup_test_db();
        let node_id = db.get_or_create_node_id().unwrap();

        db.ensure_table(
            "items",
            &[
                ("title".to_string(), "TEXT".to_string()),
                ("value".to_string(), "INTEGER".to_string()),
            ],
        )
        .unwrap();

        let data = json!({
            "title": "Test Item",
            "value": 42
        });

        let hlc = "1234567890:0:node1";
        let id = db
            .insert("items", &data, hlc, &node_id)
            .expect("Insert failed");

        let found = db.find_by_id("items", &id).expect("Find failed");
        assert!(found.is_some());

        let item = found.unwrap();
        assert_eq!(item["title"], "Test Item");
        assert_eq!(item["value"], 42);
        assert_eq!(item["_hlc"], hlc);
    }

    #[test]
    fn test_update_increments_version() {
        let db = setup_test_db();
        let node_id = db.get_or_create_node_id().unwrap();

        db.ensure_table("items", &[("title".to_string(), "TEXT".to_string())])
            .unwrap();

        let id = db
            .insert("items", &json!({"title": "Original"}), "1:0:n", &node_id)
            .unwrap();

        // 获取初始版本
        let item1 = db.find_by_id("items", &id).unwrap().unwrap();
        let v1: i64 = item1["_version"].as_i64().unwrap();

        // 更新
        db.update("items", &id, &json!({"title": "Updated"}), "2:0:n")
            .unwrap();

        // 验证版本递增
        let item2 = db.find_by_id("items", &id).unwrap().unwrap();
        let v2: i64 = item2["_version"].as_i64().unwrap();

        assert_eq!(v2, v1 + 1);
        assert_eq!(item2["title"], "Updated");
    }

    #[test]
    fn test_soft_delete() {
        let db = setup_test_db();
        let node_id = db.get_or_create_node_id().unwrap();

        db.ensure_table("items", &[("title".to_string(), "TEXT".to_string())])
            .unwrap();

        let id = db
            .insert("items", &json!({"title": "To Delete"}), "1:0:n", &node_id)
            .unwrap();

        // 确认存在
        assert!(db.find_by_id("items", &id).unwrap().is_some());

        // 软删除
        let deleted = db.delete("items", &id, "2:0:n").unwrap();
        assert!(deleted);

        // 通过 find_by_id 应该找不到（因为 _deleted = 1）
        assert!(db.find_by_id("items", &id).unwrap().is_none());
    }

    #[test]
    fn test_changelog_recording() {
        let db = setup_test_db();
        let _node_id = db.get_or_create_node_id().unwrap();

        db.ensure_table("items", &[("title".to_string(), "TEXT".to_string())])
            .unwrap();

        // 记录变更
        db.record_change(
            "items",
            "id-1",
            "INSERT",
            "1:0:n",
            Some(r#"{"title":"test"}"#),
        )
        .unwrap();
        db.record_change(
            "items",
            "id-1",
            "UPDATE",
            "2:0:n",
            Some(r#"{"title":"updated"}"#),
        )
        .unwrap();

        // 获取未同步的变更
        let changes = db.get_unsynced_changes().unwrap();
        assert_eq!(changes.len(), 2);

        // 标记为已同步
        let ids: Vec<i64> = changes.iter().map(|c| c.id).collect();
        db.mark_synced(&ids).unwrap();

        // 再次获取应该为空
        let changes2 = db.get_unsynced_changes().unwrap();
        assert!(changes2.is_empty());
    }

    #[test]
    fn test_batch_insert() {
        let db = setup_test_db();
        let node_id = db.get_or_create_node_id().unwrap();

        db.ensure_table(
            "items",
            &[
                ("title".to_string(), "TEXT".to_string()),
                ("value".to_string(), "INTEGER".to_string()),
            ],
        )
        .unwrap();

        let items = vec![
            json!({"title": "Item 1", "value": 1}),
            json!({"title": "Item 2", "value": 2}),
            json!({"title": "Item 3", "value": 3}),
        ];

        let ids = db.insert_many("items", &items, "1:0:n", &node_id).unwrap();
        assert_eq!(ids.len(), 3);

        // 验证所有记录
        let all = db.find_all("items", None, None).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn test_query_with_conditions() {
        let db = setup_test_db();
        let node_id = db.get_or_create_node_id().unwrap();

        db.ensure_table(
            "users",
            &[
                ("name".to_string(), "TEXT".to_string()),
                ("age".to_string(), "INTEGER".to_string()),
            ],
        )
        .unwrap();

        db.insert(
            "users",
            &json!({"name": "Alice", "age": 25}),
            "1:0:n",
            &node_id,
        )
        .unwrap();
        db.insert(
            "users",
            &json!({"name": "Bob", "age": 30}),
            "2:0:n",
            &node_id,
        )
        .unwrap();
        db.insert(
            "users",
            &json!({"name": "Charlie", "age": 25}),
            "3:0:n",
            &node_id,
        )
        .unwrap();

        // 条件查询
        let adults = db.find_where("users", &json!({"age": "25"})).unwrap();
        assert_eq!(adults.len(), 2);
    }

    #[test]
    fn test_advanced_query_options() {
        use crate::db::QueryOptions;

        let db = setup_test_db();
        let node_id = db.get_or_create_node_id().unwrap();

        db.ensure_table(
            "products",
            &[
                ("name".to_string(), "TEXT".to_string()),
                ("price".to_string(), "REAL".to_string()),
            ],
        )
        .unwrap();

        db.insert(
            "products",
            &json!({"name": "Apple", "price": 1.5}),
            "1:0:n",
            &node_id,
        )
        .unwrap();
        db.insert(
            "products",
            &json!({"name": "Banana", "price": 0.8}),
            "2:0:n",
            &node_id,
        )
        .unwrap();
        db.insert(
            "products",
            &json!({"name": "Cherry", "price": 3.0}),
            "3:0:n",
            &node_id,
        )
        .unwrap();

        // 模糊查询
        let options = QueryOptions {
            where_like: Some(json!({"name": "an"})),
            ..Default::default()
        };
        let results = db.query("products", &options).unwrap();
        assert_eq!(results.len(), 1); // Banana

        // 范围查询
        let options = QueryOptions {
            where_gte: Some(json!({"price": 1.0})),
            where_lte: Some(json!({"price": 2.0})),
            ..Default::default()
        };
        let results = db.query("products", &options).unwrap();
        assert_eq!(results.len(), 1); // Apple
    }

    #[test]
    fn test_count() {
        let db = setup_test_db();
        let node_id = db.get_or_create_node_id().unwrap();

        db.ensure_table("items", &[("category".to_string(), "TEXT".to_string())])
            .unwrap();

        db.insert("items", &json!({"category": "A"}), "1:0:n", &node_id)
            .unwrap();
        db.insert("items", &json!({"category": "A"}), "2:0:n", &node_id)
            .unwrap();
        db.insert("items", &json!({"category": "B"}), "3:0:n", &node_id)
            .unwrap();

        assert_eq!(db.count("items", None).unwrap(), 3);
        assert_eq!(
            db.count("items", Some(&json!({"category": "A"}))).unwrap(),
            2
        );
    }

    #[test]
    fn test_last_sync_hlc_persistence() {
        let db = setup_test_db();

        // 初始应该为空
        let hlc = db.get_last_sync_hlc("users").unwrap();
        assert!(hlc.is_none());

        // 设置
        db.set_last_sync_hlc("users", "12345:0:node1").unwrap();

        // 获取
        let hlc = db.get_last_sync_hlc("users").unwrap();
        assert_eq!(hlc, Some("12345:0:node1".to_string()));

        // 更新
        db.set_last_sync_hlc("users", "12346:0:node1").unwrap();
        let hlc = db.get_last_sync_hlc("users").unwrap();
        assert_eq!(hlc, Some("12346:0:node1".to_string()));
    }

    #[test]
    fn test_table_exists() {
        let db = setup_test_db();

        assert!(!db.table_exists("nonexistent").unwrap());

        db.ensure_table("test_table", &[("col".to_string(), "TEXT".to_string())])
            .unwrap();

        assert!(db.table_exists("test_table").unwrap());
    }

    #[test]
    fn test_list_tables() {
        let db = setup_test_db();

        db.ensure_table("table_a", &[("col".to_string(), "TEXT".to_string())])
            .unwrap();
        db.ensure_table("table_b", &[("col".to_string(), "TEXT".to_string())])
            .unwrap();

        let tables = db.list_tables().unwrap();

        // 应该不包含以 _ 开头的内部表
        assert!(tables.contains(&"table_a".to_string()));
        assert!(tables.contains(&"table_b".to_string()));
        assert!(!tables.iter().any(|t| t.starts_with('_')));
    }

    #[test]
    fn test_data_type_preservation() {
        let db = setup_test_db();
        let node_id = db.get_or_create_node_id().unwrap();

        db.ensure_table(
            "typed_data",
            &[
                ("text_col".to_string(), "TEXT".to_string()),
                ("int_col".to_string(), "INTEGER".to_string()),
                ("real_col".to_string(), "REAL".to_string()),
            ],
        )
        .unwrap();

        let data = json!({
            "text_col": "Hello",
            "int_col": 42,
            "real_col": 3.14
        });

        let id = db.insert("typed_data", &data, "1:0:n", &node_id).unwrap();
        let found = db.find_by_id("typed_data", &id).unwrap().unwrap();

        assert!(found["text_col"].is_string());
        assert!(found["int_col"].is_number());
        assert!(found["real_col"].is_number());

        assert_eq!(found["text_col"], "Hello");
        assert_eq!(found["int_col"], 42);
        // 浮点数比较
        let real_val = found["real_col"].as_f64().unwrap();
        assert!((real_val - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_unicode_data() {
        let db = setup_test_db();
        let node_id = db.get_or_create_node_id().unwrap();

        db.ensure_table(
            "unicode_test",
            &[("content".to_string(), "TEXT".to_string())],
        )
        .unwrap();

        // 测试各种 Unicode 字符
        let test_strings = vec![
            "中文测试",
            "日本語テスト",
            "한국어 테스트",
            "🎉🚀💻",
            "Mixed: Hello 世界 🌍",
        ];

        for content in &test_strings {
            let data = json!({"content": content});
            let id = db.insert("unicode_test", &data, "1:0:n", &node_id).unwrap();
            let found = db.find_by_id("unicode_test", &id).unwrap().unwrap();
            assert_eq!(found["content"].as_str().unwrap(), *content);
        }
    }

    #[test]
    fn test_null_values() {
        let db = setup_test_db();
        let node_id = db.get_or_create_node_id().unwrap();

        db.ensure_table(
            "nullable",
            &[
                ("required".to_string(), "TEXT".to_string()),
                ("optional".to_string(), "TEXT".to_string()),
            ],
        )
        .unwrap();

        let data = json!({
            "required": "value",
            "optional": null
        });

        let id = db.insert("nullable", &data, "1:0:n", &node_id).unwrap();
        let found = db.find_by_id("nullable", &id).unwrap().unwrap();

        assert_eq!(found["required"], "value");
        // null 或空字符串都可以接受
        assert!(found["optional"].is_null() || found["optional"] == "");
    }
}

SELECT
    n.nspname AS schema,
    pg_get_userbyid(d.defaclrole) AS owner_role,
    CASE d.defaclobjtype
        WHEN 'r' THEN 'TABLES'
        WHEN 'S' THEN 'SEQUENCES'
        WHEN 'f' THEN 'FUNCTIONS'
        WHEN 'T' THEN 'TYPES'
        WHEN 'n' THEN 'SCHEMAS'
    END AS object_type,
    COALESCE(r.rolname, 'PUBLIC') AS grantee,
    acl.privilege_type,
    acl.is_grantable AS grantable
FROM pg_default_acl d
JOIN pg_namespace n
    ON n.oid = d.defaclnamespace
CROSS JOIN LATERAL aclexplode(d.defaclacl) acl
LEFT JOIN pg_roles r
    ON r.oid = acl.grantee
WHERE ($1::text IS NULL OR n.nspname = $1)
ORDER BY n.nspname, owner_role, object_type, grantee, acl.privilege_type

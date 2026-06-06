SELECT
    n.nspname AS schema,
    pg_get_userbyid(d.defaclrole) AS owner_role,
    'FUNCTIONS' AS object_type,
    'PUBLIC' AS grantee,
    'EXECUTE' AS privilege_type
FROM pg_default_acl d
JOIN pg_namespace n
    ON n.oid = d.defaclnamespace
WHERE ($1::text IS NULL OR n.nspname = $1)
    AND d.defaclobjtype = 'f'
    AND NOT EXISTS (
        SELECT 1
        FROM aclexplode(d.defaclacl) acl
        WHERE acl.grantee = 0
            AND acl.privilege_type = 'EXECUTE'
    )
ORDER BY n.nspname, owner_role

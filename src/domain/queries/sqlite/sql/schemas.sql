SELECT name
FROM pragma_database_list
WHERE name NOT IN ('temp')
ORDER BY seq

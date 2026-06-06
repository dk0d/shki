SELECT extname
FROM pg_extension
WHERE extname != 'plpgsql'
ORDER BY extname

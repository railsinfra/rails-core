-- SQL Script: Get Users in Production Environment
-- 
-- This script queries users that belong to production environments.
-- Production is determined by the environments.type = 'production' column.
--
-- Usage: Connect to the users service database and run these queries.

-- ============================================
-- BASIC QUERIES
-- ============================================

-- Get all production users with environment and business details
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    e.type as environment_type,
    e.status as environment_status,
    u.created_by_user_id,
    u.created_by_api_key_id,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
ORDER BY u.created_at DESC;

-- Get only active production users
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    e.type as environment_type,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
    AND u.status = 'active'
    AND e.status = 'active'
ORDER BY u.created_at DESC;

-- ============================================
-- QUERIES BY BUSINESS
-- ============================================

-- Get production users for a specific business
-- Replace 'YOUR_BUSINESS_ID' with actual UUID
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    e.type as environment_type,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
    AND u.business_id = 'YOUR_BUSINESS_ID'::UUID
ORDER BY u.created_at DESC;

-- Get production users by business name (case-insensitive search)
-- Replace 'Business Name' with actual business name
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    e.type as environment_type,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
    AND LOWER(b.name) LIKE LOWER('%Business Name%')
ORDER BY u.created_at DESC;

-- ============================================
-- QUERIES BY ROLE
-- ============================================

-- Get all admin users in production
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
    AND u.role = 'admin'
ORDER BY u.created_at DESC;

-- Get all regular users (non-admin) in production
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
    AND u.role = 'user'
ORDER BY u.created_at DESC;

-- ============================================
-- QUERIES BY STATUS
-- ============================================

-- Get active production users
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
    AND u.status = 'active'
ORDER BY u.created_at DESC;

-- Get suspended/inactive production users
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
    AND u.status != 'active'
ORDER BY u.status, u.created_at DESC;

-- ============================================
-- SUMMARY QUERIES
-- ============================================

-- Count production users by role
SELECT 
    u.role,
    COUNT(*) as user_count,
    COUNT(DISTINCT u.business_id) as business_count
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
WHERE e.type = 'production'
GROUP BY u.role
ORDER BY user_count DESC;

-- Count production users by status
SELECT 
    u.status,
    COUNT(*) as user_count,
    COUNT(DISTINCT u.business_id) as business_count
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
WHERE e.type = 'production'
GROUP BY u.status
ORDER BY user_count DESC;

-- Count production users by business
SELECT 
    b.id as business_id,
    b.name as business_name,
    COUNT(u.id) as user_count,
    COUNT(CASE WHEN u.role = 'admin' THEN 1 END) as admin_count,
    COUNT(CASE WHEN u.role = 'user' THEN 1 END) as regular_user_count,
    COUNT(CASE WHEN u.status = 'active' THEN 1 END) as active_count
FROM businesses b
INNER JOIN users u ON u.business_id = b.id
INNER JOIN environments e ON u.environment_id = e.id
WHERE e.type = 'production'
GROUP BY b.id, b.name
ORDER BY user_count DESC;

-- ============================================
-- DETAILED QUERIES WITH PAGINATION
-- ============================================

-- Get production users with pagination (first 20 users)
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    e.type as environment_type,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
ORDER BY u.created_at DESC
LIMIT 20
OFFSET 0;

-- Get production users with pagination (next 20 users)
-- Change OFFSET to 20, 40, 60, etc. for subsequent pages
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    e.type as environment_type,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
ORDER BY u.created_at DESC
LIMIT 20
OFFSET 20;

-- ============================================
-- SEARCH QUERIES
-- ============================================

-- Search production users by email (case-insensitive)
-- Replace 'user@example.com' with search term
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
    AND LOWER(u.email) LIKE LOWER('%user@example.com%')
ORDER BY u.created_at DESC;

-- Search production users by name (first_name or last_name)
-- Replace 'John' with search term
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
    AND (LOWER(u.first_name) LIKE LOWER('%John%')
         OR LOWER(u.last_name) LIKE LOWER('%John%'))
ORDER BY u.created_at DESC;

-- ============================================
-- QUERIES WITH CREATION METADATA
-- ============================================

-- Get production users with creation metadata (who created them)
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    u.created_by_user_id,
    creator.email as created_by_user_email,
    u.created_by_api_key_id,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
LEFT JOIN users creator ON u.created_by_user_id = creator.id
WHERE e.type = 'production'
ORDER BY u.created_at DESC;

-- Get production users created by a specific admin
-- Replace 'ADMIN_USER_ID' with actual UUID
SELECT 
    u.id,
    u.email,
    u.first_name,
    u.last_name,
    u.role,
    u.status,
    u.business_id,
    b.name as business_name,
    u.environment_id,
    u.created_at,
    u.updated_at
FROM users u
INNER JOIN environments e ON u.environment_id = e.id
INNER JOIN businesses b ON u.business_id = b.id
WHERE e.type = 'production'
    AND u.created_by_user_id = 'ADMIN_USER_ID'::UUID
ORDER BY u.created_at DESC;

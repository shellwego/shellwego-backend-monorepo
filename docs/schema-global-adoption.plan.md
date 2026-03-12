# Schema Global Adoption Plan

## Document Information

| Property | Value |
|----------|-------|
| Project | ShellWeGo Backend Monorepo |
| Version | 1.0.0 |
| Status | Draft |
| Created | 2024-03-12 |
| Last Updated | 2024-03-12 |
| Author | Development Team |

---

## 1. Executive Summary

This document outlines a comprehensive plan for adopting a global schema architecture across the ShellWeGo Backend Monorepo. The initiative aims to establish a unified, scalable, and maintainable database schema strategy that supports the growing needs of our application ecosystem while ensuring data consistency, integrity, and optimal performance across all services and modules.

The global schema adoption plan addresses the critical need for standardized data models, consistent naming conventions, and shared type definitions that can be leveraged across multiple services within the monorepo. By implementing this plan, we will reduce duplication, improve developer experience, and establish a solid foundation for future scalability.

---

## 2. Current State Analysis

### 2.1 Existing Schema Structure

The current Prisma schema (`prisma/schema.prisma`) contains minimal models:

```prisma
model User {
  id        String   @id @default(cuid())
  email     String   @unique
  name      String?
  createdAt DateTime @default(now())
  updatedAt DateTime @updatedAt
}

model Post {
  id        String   @id @default(cuid())
  title     String
  content   String?
  published Boolean  @default(false)
  authorId  String
  createdAt DateTime @default(now())
  updatedAt DateTime @updatedAt
}
```

### 2.2 Identified Gaps and Issues

| Issue | Description | Impact |
|-------|-------------|--------|
| Minimal Schema | Only basic User and Post models exist | Limits application functionality |
| Missing Relations | User-Post relation not explicitly defined | Potential data integrity issues |
| No Soft Delete | Missing deletedAt fields for soft deletion | Data recovery challenges |
| No Audit Trail | Missing created/updated by fields | Compliance and debugging issues |
| No Multi-tenancy | Schema doesn't support tenant isolation | Limits SaaS capabilities |
| Missing Auth Fields | No integration with NextAuth.js | Authentication integration gap |

### 2.3 Current Database Configuration

- **Provider**: SQLite (development)
- **Client**: Prisma Client JS
- **Connection**: Environment-based DATABASE_URL
- **Migrations**: Not initialized

---

## 3. Goals and Objectives

### 3.1 Primary Goals

1. **Unified Schema Architecture**: Establish a single source of truth for all data models across the monorepo
2. **Type Safety**: Ensure end-to-end type safety from database to API to frontend
3. **Scalability**: Design schemas that support horizontal scaling and multi-tenancy
4. **Maintainability**: Create clear documentation and migration strategies

### 3.2 Success Criteria

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Schema Coverage | 100% of entities modeled | Schema audit |
| Type Safety | Zero runtime type errors | Error monitoring |
| Migration Success | 100% successful migrations | CI/CD pipeline |
| Documentation | All models documented | Documentation review |
| Performance | Query time < 100ms (p95) | Performance monitoring |

### 3.3 Non-Goals

- Database provider migration (SQLite to PostgreSQL) - addressed in separate initiative
- Legacy data migration - assumes fresh database
- Multi-region deployment - future consideration

---

## 4. Proposed Schema Architecture

### 4.1 Core Schema Design Principles

1. **Single Source of Truth**: All schema definitions reside in `prisma/schema.prisma`
2. **Consistent Naming**: snake_case for database fields, camelCase for application layer
3. **Soft Delete Pattern**: All entities support soft deletion via `deletedAt` field
4. **Audit Fields**: All entities include creation and modification tracking
5. **UUID Primary Keys**: Use CUID for all primary keys (Prisma default)
6. **Explicit Relations**: All relations are explicitly defined with proper constraints

### 4.2 Base Model Pattern

All models should inherit from a base pattern that includes:

```prisma
/// Base fields for all models
/// - id: Unique identifier (CUID)
/// - createdAt: Record creation timestamp
/// - updatedAt: Last modification timestamp
/// - deletedAt: Soft delete timestamp (null if not deleted)
/// - createdBy: User who created the record
/// - updatedBy: User who last modified the record
```

### 4.3 Proposed Schema Models

#### 4.3.1 Authentication & Authorization

```prisma
// User Model - Extended for NextAuth.js integration
model User {
  id            String    @id @default(cuid())
  email         String    @unique
  name          String?
  emailVerified DateTime?
  image         String?
  role          Role      @default(USER)
  status        Status    @default(ACTIVE)

  // Audit fields
  createdAt     DateTime  @default(now())
  updatedAt     DateTime  @updatedAt
  deletedAt     DateTime?
  createdBy     String?
  updatedBy     String?

  // Relations
  accounts      Account[]
  sessions      Session[]
  posts         Post[]
  preferences   UserPreference?

  @@index([email])
  @@index([role])
  @@index([status])
  @@softDelete(deletedAt)
}

enum Role {
  USER
  ADMIN
  MODERATOR
}

enum Status {
  ACTIVE
  INACTIVE
  SUSPENDED
  PENDING
}

// NextAuth.js required models
model Account {
  id                String  @id @default(cuid())
  userId            String
  type              String
  provider          String
  providerAccountId String
  refresh_token     String? @db.Text
  access_token      String? @db.Text
  expires_at        Int?
  token_type        String?
  scope             String?
  id_token          String? @db.Text
  session_state     String?

  createdAt DateTime @default(now())
  updatedAt DateTime @updatedAt
  deletedAt DateTime?

  user User @relation(fields: [userId], references: [id], onDelete: Cascade)

  @@unique([provider, providerAccountId])
  @@index([userId])
}

model Session {
  id           String   @id @default(cuid())
  sessionToken String   @unique
  userId       String
  expires      DateTime

  createdAt DateTime @default(now())
  updatedAt DateTime @updatedAt
  deletedAt DateTime?

  user User @relation(fields: [userId], references: [id], onDelete: Cascade)

  @@index([userId])
}

model VerificationToken {
  identifier String
  token      String   @unique
  expires    DateTime

  createdAt DateTime @default(now())

  @@unique([identifier, token])
}
```

#### 4.3.2 Content Management

```prisma
model Post {
  id          String   @id @default(cuid())
  title       String
  slug        String   @unique
  content     String?  @db.Text
  excerpt     String?  @db.Text
  published   Boolean  @default(false)
  featured    Boolean  @default(false)

  // SEO fields
  metaTitle       String?
  metaDescription String?
  keywords        String?

  // Relations
  authorId    String
  author      User     @relation(fields: [authorId], references: [id])
  categoryId  String?
  category    Category? @relation(fields: [categoryId], references: [id])
  tags        PostTag[]

  // Audit fields
  createdAt   DateTime  @default(now())
  updatedAt   DateTime  @updatedAt
  deletedAt   DateTime?
  publishedAt DateTime?
  createdBy   String?
  updatedBy   String?

  @@index([slug])
  @@index([published])
  @@index([featured])
  @@index([authorId])
  @@index([categoryId])
  @@softDelete(deletedAt)
}

model Category {
  id          String   @id @default(cuid())
  name        String
  slug        String   @unique
  description String?
  icon        String?
  color       String?

  // Relations
  posts       Post[]
  parentId    String?
  parent      Category?  @relation("CategoryHierarchy", fields: [parentId], references: [id])
  children    Category[] @relation("CategoryHierarchy")

  // Audit fields
  createdAt DateTime @default(now())
  updatedAt DateTime @updatedAt
  deletedAt DateTime?

  @@index([slug])
  @@softDelete(deletedAt)
}

model Tag {
  id          String   @id @default(cuid())
  name        String   @unique
  slug        String   @unique
  description String?

  // Relations
  posts       PostTag[]

  // Audit fields
  createdAt DateTime @default(now())
  updatedAt DateTime @updatedAt
  deletedAt DateTime?

  @@index([slug])
  @@softDelete(deletedAt)
}

model PostTag {
  postId    String
  tagId     String

  post      Post @relation(fields: [postId], references: [id], onDelete: Cascade)
  tag       Tag  @relation(fields: [tagId], references: [id], onDelete: Cascade)

  createdAt DateTime @default(now())

  @@id([postId, tagId])
}
```

#### 4.3.3 User Preferences & Settings

```prisma
model UserPreference {
  id                    String   @id @default(cuid())
  userId                String   @unique

  // Notification preferences
  emailNotifications    Boolean  @default(true)
  pushNotifications     Boolean  @default(false)
  marketingEmails       Boolean  @default(false)

  // Display preferences
  theme                 String   @default("system")
  language              String   @default("en")
  timezone              String   @default("UTC")

  // Privacy settings
  profileVisibility     String   @default("public")
  showEmail             Boolean  @default(false)

  // JSON blob for extensibility
  customPreferences     String?  @db.Text

  // Relations
  user                  User     @relation(fields: [userId], references: [id], onDelete: Cascade)

  // Audit fields
  createdAt             DateTime @default(now())
  updatedAt             DateTime @updatedAt
  deletedAt             DateTime?

  @@index([userId])
}
```

#### 4.3.4 Audit & Logging

```prisma
model AuditLog {
  id          String   @id @default(cuid())
  action      String
  entityType  String
  entityId    String
  userId      String?
  ipAddress   String?
  userAgent   String?
  changes     String?  @db.Text  // JSON diff of changes

  createdAt   DateTime @default(now())

  @@index([entityType, entityId])
  @@index([userId])
  @@index([action])
  @@index([createdAt])
}

model SystemLog {
  id          String   @id @default(cuid())
  level       LogLevel
  message     String   @db.Text
  context     String?  @db.Text  // JSON context data
  source      String?
  stackTrace  String?  @db.Text

  createdAt   DateTime @default(now())

  @@index([level])
  @@index([source])
  @@index([createdAt])
}

enum LogLevel {
  DEBUG
  INFO
  WARN
  ERROR
  FATAL
}
```

### 4.4 Schema File Organization

```
prisma/
├── schema.prisma          # Main schema file (imports all others)
├── schemas/
│   ├── auth.prisma        # Authentication models
│   ├── content.prisma     # Content management models
│   ├── user.prisma        # User-related models
│   ├── audit.prisma       # Audit & logging models
│   └── _base.prisma       # Shared types and enums
├── migrations/            # Migration history
└── seed.ts               # Database seeding
```

---

## 5. Migration Strategy

### 5.1 Migration Approach

We will adopt an **iterative migration approach** with the following phases:

1. **Phase 1: Foundation** - Establish base models and patterns
2. **Phase 2: Core Features** - Migrate essential business models
3. **Phase 3: Extended Features** - Add supporting models
4. **Phase 4: Optimization** - Add indexes and constraints

### 5.2 Migration Workflow

```
┌─────────────────────────────────────────────────────────────────┐
│                    MIGRATION WORKFLOW                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. Schema Update                                               │
│     └──> Modify schema.prisma                                   │
│                                                                 │
│  2. Validation                                                  │
│     └──> prisma validate                                        │
│     └──> prisma format                                          │
│                                                                 │
│  3. Migration Generation                                        │
│     └──> prisma migrate dev --name <migration_name>             │
│                                                                 │
│  4. Review Generated SQL                                        │
│     └──> Check prisma/migrations/<timestamp>/migration.sql      │
│                                                                 │
│  5. Testing                                                     │
│     └──> Run test suite                                         │
│     └──> Verify data integrity                                  │
│                                                                 │
│  6. Commit & Push                                               │
│     └──> Commit schema + migration files                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 5.3 Migration Naming Convention

```
<timestamp>_<action>_<entity>

Examples:
- 20240312000000_add_user_preferences
- 20240312000001_extend_post_model
- 20240312000002_add_audit_logs
- 20240312000003_add_indexes
```

### 5.4 Rollback Strategy

Each migration should include:
1. Forward migration (up.sql)
2. Rollback migration (down.sql) - if supported
3. Data migration scripts for complex changes

**Important**: SQLite has limited ALTER TABLE support. For production, consider:
- PostgreSQL for full migration support
- Blue-green deployment strategy
- Zero-downtime migration patterns

---

## 6. Implementation Phases

### Phase 1: Foundation (Week 1-2)

| Task | Description | Status |
|------|-------------|--------|
| P1-01 | Create schema file organization structure | Pending |
| P1-02 | Define base model pattern and mixins | Pending |
| P1-03 | Implement User model with NextAuth integration | Pending |
| P1-04 | Implement Account and Session models | Pending |
| P1-05 | Add VerificationToken model | Pending |
| P1-06 | Create initial migration | Pending |
| P1-07 | Set up database seeding | Pending |
| P1-08 | Update db.ts with global Prisma pattern | Pending |

### Phase 2: Core Features (Week 3-4)

| Task | Description | Status |
|------|-------------|--------|
| P2-01 | Extend Post model with full fields | Pending |
| P2-02 | Implement Category model with hierarchy | Pending |
| P2-03 | Implement Tag model | Pending |
| P2-04 | Create PostTag junction table | Pending |
| P2-05 | Add UserPreference model | Pending |
| P2-06 | Create migrations for all models | Pending |
| P2-07 | Implement CRUD operations in API routes | Pending |
| P2-08 | Add Zod validation schemas | Pending |

### Phase 3: Extended Features (Week 5-6)

| Task | Description | Status |
|------|-------------|--------|
| P3-01 | Implement AuditLog model | Pending |
| P3-02 | Implement SystemLog model | Pending |
| P3-03 | Add audit middleware for automatic logging | Pending |
| P3-04 | Create admin dashboard models | Pending |
| P3-05 | Add notification models | Pending |
| P3-06 | Implement file/attachment models | Pending |
| P3-07 | Create analytics models | Pending |
| P3-08 | Add comprehensive indexes | Pending |

### Phase 4: Optimization & Polish (Week 7-8)

| Task | Description | Status |
|------|-------------|--------|
| P4-01 | Performance audit and optimization | Pending |
| P4-02 | Add missing indexes | Pending |
| P4-03 | Implement query optimization patterns | Pending |
| P4-04 | Create database backup strategy | Pending |
| P4-05 | Document all models and relations | Pending |
| P4-06 | Create API documentation | Pending |
| P4-07 | Load testing and optimization | Pending |
| P4-08 | Final review and sign-off | Pending |

---

## 7. Type Safety Implementation

### 7.1 Prisma Type Generation

```bash
# Generate Prisma client types
bun run db:generate
```

### 7.2 Zod Schema Integration

Create type-safe validation schemas in `src/lib/validations/`:

```typescript
// src/lib/validations/user.ts
import { z } from 'zod';

export const userCreateSchema = z.object({
  email: z.string().email(),
  name: z.string().min(1).max(100).optional(),
  role: z.enum(['USER', 'ADMIN', 'MODERATOR']).default('USER'),
});

export const userUpdateSchema = userCreateSchema.partial();

export type UserCreateInput = z.infer<typeof userCreateSchema>;
export type UserUpdateInput = z.infer<typeof userUpdateSchema>;
```

### 7.3 API Response Types

```typescript
// src/lib/types/api.ts
import { User, Post, Category } from '@prisma/client';

export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: {
    code: string;
    message: string;
  };
  meta?: {
    page?: number;
    limit?: number;
    total?: number;
  };
}

export interface UserWithRelations extends User {
  posts?: Post[];
  preferences?: UserPreference | null;
}

export interface PostWithRelations extends Post {
  author?: User;
  category?: Category | null;
  tags?: Tag[];
}
```

---

## 8. Testing Strategy

### 8.1 Unit Tests

- Model creation/validation tests
- Schema validation tests
- Type guard tests

### 8.2 Integration Tests

- API endpoint tests with database
- Migration tests
- Seed data validation

### 8.3 Performance Tests

- Query performance benchmarks
- Index effectiveness tests
- Load testing scenarios

### 8.4 Test Database Configuration

```typescript
// vitest.config.ts
export default defineConfig({
  test: {
    environment: 'node',
    setupFiles: ['./tests/setup.ts'],
    globalSetup: ['./tests/globalSetup.ts'],
  },
});
```

---

## 9. Documentation Requirements

### 9.1 Schema Documentation

Each model must include:
- Purpose and description
- Field descriptions
- Relation explanations
- Usage examples
- Constraints and validations

### 9.2 API Documentation

- OpenAPI/Swagger specification
- Request/response schemas
- Authentication requirements
- Error codes and handling

### 9.3 Migration Documentation

- Migration history
- Breaking changes log
- Rollback procedures
- Data migration scripts

---

## 10. Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Migration failure | Low | High | Test in staging, backup strategy |
| Performance degradation | Medium | High | Load testing, query optimization |
| Data inconsistency | Low | Critical | Transactions, constraints |
| Breaking API changes | Medium | High | Versioning, deprecation notices |
| Developer confusion | Medium | Medium | Clear documentation, training |

---

## 11. Dependencies and Prerequisites

### 11.1 Technical Dependencies

- Prisma CLI ^6.11.1 (installed)
- PostgreSQL (recommended for production)
- Node.js 18+ / Bun runtime

### 11.2 Team Dependencies

- Developer training on Prisma
- Code review bandwidth
- QA testing resources

### 11.3 Infrastructure Dependencies

- CI/CD pipeline for migrations
- Staging environment
- Backup infrastructure

---

## 12. Appendices

### Appendix A: Migration Commands Reference

```bash
# Development
bun run db:generate          # Generate Prisma client
bun run db:push              # Push schema changes (dev only)
bun run db:migrate           # Create and apply migration
bun run db:reset             # Reset database with seed

# Production
prisma migrate deploy        # Apply pending migrations
prisma migrate resolve       # Resolve migration issues
prisma db execute            # Execute raw SQL
```

### Appendix B: Prisma Client Best Practices

```typescript
// Use global Prisma instance pattern
import { db } from '@/lib/db';

// Use transactions for related operations
await db.$transaction([
  db.user.create({ data: userData }),
  db.userPreference.create({ data: preferenceData }),
]);

// Use select for performance
const user = await db.user.findUnique({
  where: { id },
  select: {
    id: true,
    email: true,
    name: true,
    // Exclude sensitive fields
  },
});

// Use include for relations
const userWithPosts = await db.user.findUnique({
  where: { id },
  include: {
    posts: {
      where: { published: true },
      take: 10,
    },
  },
});
```

### Appendix C: Environment Variables

```bash
# .env
DATABASE_URL="file:./dev.db"

# Production
DATABASE_URL="postgresql://user:password@host:5432/shellwego?schema=public"

# Shadow database for migrations (optional)
SHADOW_DATABASE_URL="postgresql://user:password@host:5432/shellwego_shadow?schema=public"
```

---

## 13. Status Tracking

### Current Progress

- [x] Document created
- [ ] Phase 1 tasks
- [ ] Phase 2 tasks
- [ ] Phase 3 tasks
- [ ] Phase 4 tasks

### Next Steps

1. Review and approve this plan
2. Initialize Prisma migrations
3. Begin Phase 1 implementation
4. Schedule weekly progress reviews

---

*This document is a living document and should be updated as the project progresses.*

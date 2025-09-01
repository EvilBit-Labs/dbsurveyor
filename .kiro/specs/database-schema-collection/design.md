# Database Schema Collection - Design Document

## Overview

The database schema collection and documentation system implements a dual-binary architecture with a collector (dbsurveyor-collect) and postprocessor (dbsurveyor). The system provides secure, offline-capable database introspection across multiple database engines including relational databases (PostgreSQL, MySQL, SQLite, SQL Server, Oracle), NoSQL databases (MongoDB, Cassandra), and columnar databases (ClickHouse, BigQuery).

The design emphasizes security-first principles with zero telemetry, offline operation, and comprehensive credential protection while maintaining high performance and extensibility through a plugin architecture. The collector NEVER alters, redacts, or modifies data during collection - it stores samples exactly as they are collected from the database. Redaction and privacy controls are exclusively handled by the postprocessor.

## Modular Design Documentation

This design document is organized into focused modules for better maintainability. Each section is detailed in separate documents:

### Core Architecture and Components

- **Architecture**: #[[file:design/architecture.md]] - High-level system architecture and dual-binary design
- **Components**: #[[file:design/components.md]] - Core components and interfaces (CLI, templates, logging)  
- **Adapters**: #[[file:design/adapters.md]] - Database adapter system and implementations

### Data Models and Schema

- **Data Models**: #[[file:design/data-models.md]] - Unified schema representation and data structures
- **JSON Schema**: #[[file:design/json-schema.md]] - JSON Schema specification for output format validation
- **Type System**: #[[file:design/type-system.md]] - Unified data type mapping across databases

### Security and Performance  

- **Security**: #[[file:design/security.md]] - Security architecture, credential management, and encryption
- **Plugin Architecture**: #[[file:design/plugin-architecture.md]] - Plugin system design and WASM integration
- **Testing Strategy**: #[[file:design/testing-strategy.md]] - Comprehensive testing framework and approaches

### Operations and Distribution

- **Error Handling**: #[[file:design/error-handling.md]] - Error handling patterns and security considerations
- **Distribution**: #[[file:design/distribution.md]] - Build configuration, release management, and specialized binaries
- **Documentation**: #[[file:design/documentation.md]] - Documentation architecture and standards

## Design Principles

The system follows these core principles:

1. **Security-First**: Every design decision prioritizes security and privacy
2. **Offline-Capable**: Zero external dependencies after database connection  
3. **Database-Agnostic**: Unified interface across all supported databases
4. **Dual-Binary Architecture**: Separate collector and postprocessor for flexible workflows

## Quick Reference

For implementation details, see the modular design documents above. Key highlights:

- **JSON Schema Specification**: #[[file:design/json-schema.md]] defines the comprehensive output format based on Frictionless Data Table Schema
- **Security Architecture**: #[[file:design/security.md]] covers credential protection and encryption requirements
- **Database Adapters**: #[[file:design/adapters.md]] details the unified adapter system for all supported databases
- **Testing Strategy**: #[[file:design/testing-strategy.md]] outlines the multi-layered testing approach with testcontainers

## Implementation Status

Current implementation progress can be tracked in the [tasks.md](tasks.md) file. Key completed components:

- ✅ **Core Architecture**: Dual-binary workspace structure established
- ✅ **Security Foundation**: AES-GCM encryption and Argon2id key derivation implemented  
- ✅ **PostgreSQL Adapter**: Schema collection, foreign key mapping, and data sampling
- ✅ **CLI Framework**: Comprehensive command-line interfaces for both binaries
- 🚧 **JSON Schema Specification**: In progress (Task 2.6)
- 🚧 **Testing Framework**: Testcontainers integration in progress

## Next Steps

The immediate focus is on completing the JSON Schema specification (Task 2.6) which will provide:

1. **Validation Foundation**: Ensure consistent output format across all database adapters
2. **Documentation Standard**: Clear specification for postprocessor input requirements  
3. **Quality Assurance**: Prevent malformed output through schema validation
4. **Future Compatibility**: Version management for format evolution

See #[[file:design/json-schema.md]] for detailed specification requirements.

## Navigation

- **Requirements**: [requirements.md](requirements.md) - User stories and acceptance criteria
- **Tasks**: [tasks.md](tasks.md) - Implementation plan and progress tracking
- **Design Modules**: [design/README.md](design/README.md) - Detailed design documentation index

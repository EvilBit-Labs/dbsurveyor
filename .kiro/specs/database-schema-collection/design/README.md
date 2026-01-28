# Database Schema Collection - Design Documentation

This directory contains the modular design documentation for the DBSurveyor database schema collection system.

## Document Structure

The design is organized into focused documents for better maintainability:

### Core Architecture

- **[architecture.md](architecture.md)** - High-level system architecture and dual-binary design
- **[components.md](components.md)** - Core components and interfaces (CLI, templates, logging)
- **[adapters.md](adapters.md)** - Database adapter system and implementations

### Data and Schema

- **[data-models.md](data-models.md)** - Unified schema representation and data structures
- **[json-schema.md](json-schema.md)** - JSON Schema specification for output format validation
- **[type-system.md](type-system.md)** - Unified data type mapping across databases

### Security and Performance

- **[security.md](security.md)** - Security architecture, credential management, and encryption
- **[plugin-architecture.md](plugin-architecture.md)** - Plugin system design and WASM integration
- **[testing-strategy.md](testing-strategy.md)** - Comprehensive testing framework and approaches

### Operations and Distribution

- **[error-handling.md](error-handling.md)** - Error handling patterns and security considerations
- **[distribution.md](distribution.md)** - Build configuration, release management, and specialized binaries
- **[documentation.md](documentation.md)** - Documentation architecture and standards

## Design Principles

The system follows these core principles:

1. **Security-First**: Every design decision prioritizes security and privacy
2. **Offline-Capable**: Zero external dependencies after database connection
3. **Database-Agnostic**: Unified interface across all supported databases
4. **Dual-Binary Architecture**: Separate collector and postprocessor for flexible workflows

## Quick Navigation

- **Getting Started**: Start with [architecture.md](architecture.md) for system overview
- **Implementation**: See [adapters.md](adapters.md) for database-specific details
- **Security**: Review [security.md](security.md) for security requirements
- **Testing**: Check [testing-strategy.md](testing-strategy.md) for testing approaches
- **Schema Format**: See [json-schema.md](json-schema.md) for output format specification

## File References

This modular structure uses `#[[file:relative_path]]` references to include content from other files, enabling:

- **Focused Documentation**: Each file covers a specific aspect of the system
- **Better Maintainability**: Easier to update and review individual components
- **Cross-References**: Clear relationships between different design aspects
- **Reusable Content**: Shared definitions and examples across documents

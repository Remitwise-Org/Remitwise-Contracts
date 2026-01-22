# Architecture Documentation

## Overview

The RemitWise smart contracts provide a comprehensive financial management system for remittance recipients. The architecture is designed to automatically allocate funds, manage recurring obligations, and support long-term financial goals.

## System Architecture

```mermaid
graph TB
    A[Remittance Received] --> B[Remittance Split Contract]
    B --> C[Spending Pool]
    B --> D[Savings Goals Contract]
    B --> E[Bill Payments Contract]
    B --> F[Insurance Contract]

    D --> G[Education Goal]
    D --> H[Medical Goal]
    D --> I[Emergency Fund]

    E --> J[Electricity Bill]
    E --> K[School Fees]
    E --> L[Water Bill]

    F --> M[Health Insurance]
    F --> N[Emergency Insurance]
    F --> O[Life Insurance]

    P[User Interface] --> B
    P --> D
    P --> E
    P --> F

    Q[Payment Processor] --> A
```

## Contract Relationships

### Core Contracts

```mermaid
graph LR
    A[Remittance Split] --> B[Bill Payments]
    A --> C[Insurance]
    A --> D[Savings Goals]

    B --> E[Recurring Bills]
    C --> F[Policy Management]
    D --> G[Goal Tracking]

    H[External Systems] --> A
    H --> I[Payment APIs]
    H --> J[Banking Systems]
```

### Data Flow

```mermaid
sequenceDiagram
    participant U as User
    participant RP as Remittance Processor
    participant RS as Remittance Split
    participant BP as Bill Payments
    participant IN as Insurance
    participant SG as Savings Goals

    U->>RP: Send Remittance
    RP->>RS: Process Payment
    RS->>RS: Calculate Split
    RS->>BP: Allocate to Bills
    RS->>IN: Allocate to Insurance
    RS->>SG: Allocate to Savings
    BP-->>U: Bill Payment Confirmations
    IN-->>U: Premium Payment Confirmations
    SG-->>U: Savings Updates
```

## Contract Details

### Remittance Split Contract

**Purpose**: Automatically divides incoming remittances into predefined categories.

**Key Features**:
- Configurable percentage allocation
- Default fallback configuration
- Real-time split calculations

**Integration Points**:
- Receives total remittance amount
- Outputs allocation amounts
- No dependencies on other contracts

```mermaid
classDiagram
    class RemittanceSplit {
        +initialize_split(percentages) bool
        +get_split() Vec<u32>
        +calculate_split(amount) Vec<i128>
    }
```

### Bill Payments Contract

**Purpose**: Manages bill tracking, payment scheduling, and recurring payments.

**Key Features**:
- Recurring bill automation
- Payment tracking
- Due date management

**Integration Points**:
- Receives allocation from Remittance Split
- Stores bill data persistently
- Generates payment notifications

```mermaid
classDiagram
    class BillPayments {
        +create_bill(details) u32
        +pay_bill(id) bool
        +get_bill(id) Option<Bill>
        +get_unpaid_bills() Vec<Bill>
        +get_total_unpaid() i128
    }

    class Bill {
        +id: u32
        +name: String
        +amount: i128
        +due_date: u64
        +recurring: bool
        +frequency_days: u32
        +paid: bool
    }

    BillPayments --> Bill
```

### Insurance Contract

**Purpose**: Manages micro-insurance policies and premium payments.

**Key Features**:
- Policy lifecycle management
- Premium payment scheduling
- Coverage tracking

**Integration Points**:
- Receives allocation from Remittance Split
- Manages policy data
- Handles premium collections

```mermaid
classDiagram
    class Insurance {
        +create_policy(details) u32
        +pay_premium(id) bool
        +get_policy(id) Option<InsurancePolicy>
        +get_active_policies() Vec<InsurancePolicy>
        +get_total_monthly_premium() i128
        +deactivate_policy(id) bool
    }

    class InsurancePolicy {
        +id: u32
        +name: String
        +coverage_type: String
        +monthly_premium: i128
        +coverage_amount: i128
        +active: bool
        +next_payment_date: u64
    }

    Insurance --> InsurancePolicy
```

### Savings Goals Contract

**Purpose**: Manages goal-based savings with target dates and progress tracking.

**Key Features**:
- Goal creation and tracking
- Fund allocation
- Completion monitoring

**Integration Points**:
- Receives allocation from Remittance Split
- Tracks savings progress
- Provides goal status updates

```mermaid
classDiagram
    class SavingsGoals {
        +create_goal(details) u32
        +add_to_goal(id, amount) i128
        +get_goal(id) Option<SavingsGoal>
        +get_all_goals() Vec<SavingsGoal>
        +is_goal_completed(id) bool
    }

    class SavingsGoal {
        +id: u32
        +name: String
        +target_amount: i128
        +current_amount: i128
        +target_date: u64
        +locked: bool
    }

    SavingsGoals --> SavingsGoal
```

## Integration Patterns

### Frontend Integration

```mermaid
graph TB
    A[Web/Mobile App] --> B[API Gateway]
    B --> C[Contract Abstraction Layer]
    C --> D[Remittance Split]
    C --> E[Bill Payments]
    C --> F[Insurance]
    C --> G[Savings Goals]

    H[Wallet Integration] --> B
    I[Payment Processor] --> B
    J[Notification Service] --> B
```

### Batch Processing Pattern

```mermaid
flowchart TD
    A[Remittance Received] --> B{Validate Amount}
    B --> C[Calculate Split]
    C --> D[Process Bills]
    C --> E[Process Insurance]
    C --> F[Process Savings]

    D --> G{Check Success}
    E --> G
    F --> G

    G --> H[Send Notifications]
    G --> I[Update Dashboard]
```

### Error Handling Pattern

```mermaid
flowchart TD
    A[Contract Call] --> B{Execute}
    B --> C{Success?}
    C --> D[Return Result]
    C --> E[Handle Error]

    E --> F{Retryable?}
    F --> G[Retry with Backoff]
    F --> H[Log Error]

    G --> B
    H --> I[Notify User]
    I --> J[Continue Processing]
```

## Security Considerations

### Access Control

```mermaid
graph LR
    A[User] --> B[Authentication]
    B --> C[Authorization]
    C --> D{Contract Access}
    D --> E[Execute Function]
    D --> F[Access Denied]

    G[Admin] --> B
    H[Contract Owner] --> B
```

### Data Validation

- Input sanitization at contract boundaries
- Amount validation (positive values)
- Date validation (future dates for goals/bills)
- Percentage validation (sum to 100%)

### Storage Security

- Persistent data encryption where applicable
- Access control for sensitive operations
- Audit trail for financial transactions

## Performance Characteristics

### Gas Optimization

- Efficient data structures (Vec, Map)
- Minimal storage operations
- Batch processing for multiple items

### Scalability

- Contract isolation prevents cascading failures
- Horizontal scaling through multiple deployments
- Event-driven architecture for notifications

## Deployment Architecture

```mermaid
graph TB
    A[Development] --> B[Testing]
    B --> C[Staging]
    C --> D[Production]

    E[Soroban CLI] --> F[Testnet Deployment]
    F --> G[Contract Verification]
    G --> H[Mainnet Deployment]

    I[Monitoring] --> J[Performance Metrics]
    I --> K[Error Tracking]
    I --> L[Usage Analytics]
```

## Monitoring and Observability

### Key Metrics

- Contract invocation frequency
- Gas usage per function
- Error rates by contract
- User adoption rates
- Financial transaction volumes

### Logging Strategy

- Structured logging for all contract calls
- Error categorization and alerting
- Performance monitoring
- Audit trail generation

## Future Extensions

### Planned Enhancements

```mermaid
mindmap
  root((RemitWise Contracts))
    Family Wallet
      Multi-user access
      Spending limits
      Permission management
    Advanced Analytics
      Spending patterns
      Goal progress prediction
      Financial health scoring
    Cross-chain Support
      Multi-network deployment
      Bridge integration
      Asset management
    DeFi Integration
      Yield farming
      Lending protocols
      Insurance markets
```

### Integration Points

- Banking APIs for direct bill payment
- Insurance providers for policy management
- Financial planning tools
- Remittance service providers
- Mobile money platforms
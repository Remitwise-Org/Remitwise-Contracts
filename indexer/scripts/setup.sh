#!/bin/bash
# Quick setup script for the Remitwise indexer

set -e

echo "=== Remitwise Indexer Setup ==="
echo ""

# Check Node.js version
if ! command -v node &> /dev/null; then
    echo "Error: Node.js is not installed"
    echo "Please install Node.js 18+ from https://nodejs.org/"
    exit 1
fi

NODE_VERSION=$(node -v | cut -d'v' -f2 | cut -d'.' -f1)
if [ "$NODE_VERSION" -lt 18 ]; then
    echo "Error: Node.js version 18+ is required (found: $(node -v))"
    exit 1
fi

echo "✓ Node.js $(node -v) detected"

# Install dependencies
echo ""
echo "Installing dependencies..."
npm install

# Create .env if it doesn't exist
if [ ! -f .env ]; then
    echo ""
    echo "Creating .env file from template..."
    cp .env.example .env
    echo "✓ Created .env file"
    echo ""
    echo "⚠️  IMPORTANT: Edit .env and configure your contract addresses!"
    echo "   Required variables:"
    echo "   - STELLAR_RPC_URL"
    echo "   - BILL_PAYMENTS_CONTRACT"
    echo "   - SAVINGS_GOALS_CONTRACT"
    echo "   - INSURANCE_CONTRACT"
else
    echo "✓ .env file already exists"
fi

# Create data directory
mkdir -p data
echo "✓ Created data directory"

# Build the project
echo ""
echo "Building TypeScript..."
npm run build
echo "✓ Build complete"

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Next steps:"
echo "  1. Edit .env with your contract addresses"
echo "  2. Run: npm start"
echo ""
echo "For queries:"
echo "  npm start query dashboard <address>"
echo "  npm start query overdue"
echo "  npm start query tags"
echo ""

# Deployment Checklist

Use this checklist when deploying the Remitwise indexer to production or testing environments.

## Pre-Deployment

### Environment Setup

- [ ] Node.js 18+ installed
- [ ] npm or yarn installed
- [ ] Access to Stellar RPC endpoint
- [ ] Contract addresses available
- [ ] Database storage location identified

### Configuration

- [ ] Copy `.env.example` to `.env`
- [ ] Set `STELLAR_RPC_URL` (testnet or mainnet)
- [ ] Set `NETWORK_PASSPHRASE` correctly
- [ ] Configure all contract addresses:
  - [ ] `BILL_PAYMENTS_CONTRACT`
  - [ ] `SAVINGS_GOALS_CONTRACT`
  - [ ] `INSURANCE_CONTRACT`
  - [ ] `REMITTANCE_SPLIT_CONTRACT` (optional)
- [ ] Set `DB_PATH` for database location
- [ ] Configure `POLL_INTERVAL_MS` (default: 5000)
- [ ] Set `START_LEDGER` appropriately:
  - [ ] 0 for full history
  - [ ] Specific ledger for partial sync
  - [ ] Current ledger for new events only

### Dependencies

- [ ] Run `npm install`
- [ ] Verify no security vulnerabilities: `npm audit`
- [ ] Build project: `npm run build`
- [ ] Verify build output in `dist/` directory

## Testing

### Local Testing

- [ ] Test with localnet:
  - [ ] Start Stellar localnet: `stellar network start local`
  - [ ] Deploy contracts to localnet
  - [ ] Configure indexer for localnet
  - [ ] Run indexer: `npm start`
  - [ ] Generate test events
  - [ ] Verify events are indexed
  - [ ] Test query commands

### Query Testing

- [ ] Test user dashboard query
- [ ] Test overdue bills query
- [ ] Test tag filtering
- [ ] Test all tags query
- [ ] Test active goals query
- [ ] Verify query performance (<100ms)

### Database Testing

- [ ] Verify database file created
- [ ] Check database size is reasonable
- [ ] Verify indexes are created
- [ ] Test database backup/restore
- [ ] Test database reset script

## Deployment

### Production Environment

- [ ] Choose deployment method:
  - [ ] Docker Compose
  - [ ] Manual deployment
  - [ ] Cloud service (AWS, GCP, Azure)
  - [ ] Kubernetes

### Docker Deployment

- [ ] Build Docker image: `docker build -t remitwise-indexer .`
- [ ] Test image locally: `docker run --env-file .env remitwise-indexer`
- [ ] Configure `docker-compose.yml`
- [ ] Set up volume for database persistence
- [ ] Start services: `docker-compose up -d`
- [ ] Verify container is running: `docker-compose ps`
- [ ] Check logs: `docker-compose logs -f indexer`

### Manual Deployment

- [ ] Install production dependencies: `npm ci --only=production`
- [ ] Build project: `npm run build`
- [ ] Set up process manager (PM2, systemd)
- [ ] Configure auto-restart on failure
- [ ] Set up log rotation
- [ ] Start indexer
- [ ] Verify process is running

### Cloud Deployment

- [ ] Provision compute instance
- [ ] Configure security groups/firewall
- [ ] Set up persistent storage for database
- [ ] Configure environment variables
- [ ] Deploy application
- [ ] Set up monitoring
- [ ] Configure backups

## Post-Deployment

### Verification

- [ ] Indexer is running without errors
- [ ] Events are being processed
- [ ] Database is being updated
- [ ] Last processed ledger is advancing
- [ ] Query commands work correctly
- [ ] No memory leaks observed
- [ ] CPU usage is acceptable
- [ ] Disk usage is growing as expected

### Monitoring Setup

- [ ] Set up log aggregation
- [ ] Configure alerting for:
  - [ ] Indexer process down
  - [ ] No events processed in X minutes
  - [ ] Database errors
  - [ ] Disk space low
  - [ ] Memory usage high
- [ ] Set up metrics collection:
  - [ ] Events processed per minute
  - [ ] Query latency
  - [ ] Database size
  - [ ] Last processed ledger

### Backup Configuration

- [ ] Set up automated database backups
- [ ] Test backup restoration
- [ ] Configure backup retention policy
- [ ] Document backup location
- [ ] Set up off-site backup storage

## Maintenance

### Regular Tasks

- [ ] Monitor logs for errors
- [ ] Check database size growth
- [ ] Verify event processing is current
- [ ] Review query performance
- [ ] Check for npm package updates
- [ ] Review security advisories

### Weekly Tasks

- [ ] Review error logs
- [ ] Check disk space
- [ ] Verify backups are working
- [ ] Test query performance
- [ ] Review monitoring alerts

### Monthly Tasks

- [ ] Update dependencies (if needed)
- [ ] Review and optimize queries
- [ ] Analyze database growth trends
- [ ] Test disaster recovery procedures
- [ ] Review and update documentation

## Troubleshooting

### Common Issues Checklist

- [ ] Indexer not starting:
  - [ ] Check environment variables
  - [ ] Verify Node.js version
  - [ ] Check database permissions
  - [ ] Review error logs

- [ ] No events being processed:
  - [ ] Verify RPC endpoint is accessible
  - [ ] Check contract addresses are correct
  - [ ] Verify START_LEDGER is appropriate
  - [ ] Check network connectivity

- [ ] Database errors:
  - [ ] Check disk space
  - [ ] Verify file permissions
  - [ ] Check for database corruption
  - [ ] Review WAL mode settings

- [ ] High memory usage:
  - [ ] Check for memory leaks
  - [ ] Review batch sizes
  - [ ] Monitor event processing rate
  - [ ] Consider restarting indexer

- [ ] Slow queries:
  - [ ] Verify indexes are created
  - [ ] Check database size
  - [ ] Review query patterns
  - [ ] Consider database optimization

## Rollback Plan

### If Issues Occur

- [ ] Stop indexer immediately
- [ ] Identify the issue
- [ ] Check logs for errors
- [ ] Restore from backup if needed
- [ ] Revert to previous version if necessary
- [ ] Document the issue
- [ ] Fix and redeploy

### Rollback Steps

1. [ ] Stop current indexer
2. [ ] Restore database from backup
3. [ ] Deploy previous version
4. [ ] Verify functionality
5. [ ] Monitor for issues
6. [ ] Document lessons learned

## Security

### Security Checklist

- [ ] Environment variables are secure
- [ ] Database file permissions are restricted
- [ ] No sensitive data in logs
- [ ] RPC endpoint uses HTTPS
- [ ] Regular security updates applied
- [ ] Access to production is restricted
- [ ] Audit logs are enabled

## Documentation

### Documentation Checklist

- [ ] Deployment procedure documented
- [ ] Configuration options documented
- [ ] Troubleshooting guide updated
- [ ] Monitoring setup documented
- [ ] Backup/restore procedures documented
- [ ] Contact information for support
- [ ] Runbook for common issues

## Sign-off

### Deployment Approval

- [ ] Technical lead approval
- [ ] Security review completed
- [ ] Testing completed successfully
- [ ] Documentation reviewed
- [ ] Monitoring configured
- [ ] Backup strategy approved
- [ ] Rollback plan reviewed

### Post-Deployment Review

- [ ] Deployment successful
- [ ] All checks passed
- [ ] Team notified
- [ ] Documentation updated
- [ ] Lessons learned documented

---

**Deployment Date**: _______________

**Deployed By**: _______________

**Environment**: [ ] Testnet [ ] Mainnet [ ] Localnet

**Version**: _______________

**Notes**:
_______________________________________________
_______________________________________________
_______________________________________________

# Docker Containerization Guide

This project is containerized using Docker for easy deployment and scaling. The application consists of main services: frontend (Next.js) and backend (FastAPI with SQLite database).

## Prerequisites

- Docker and Docker Compose installed
- Git
- At least 2GB RAM available

## Quick Start

### Development Environment

```bash
# Copy environment variables (if not already present)
cp .env.example .env

# Start all services in development mode
docker-compose up --build

# Or start in detached mode
docker-compose up --build -d

# View logs
docker-compose logs -f

# Stop services
docker-compose down
```

### Production Environment

```bash
# Copy production environment variables
cp .env.example .env.prod

# Start in production mode
docker-compose -f docker-compose.yml -f docker-compose.prod.yml up --build -d
```

## Service Architecture

### Services Overview

1. **backend** - FastAPI application
   - Port: 8000
   - Multi-stage build for optimization
   - Health checks and logging
   - SQLite database persistence using volume

2. **frontend** - Next.js application
   - Port: 3000
   - Multi-stage build with static optimization
   - Optimized for production

3. **nginx** - Reverse proxy and load balancer
   - Ports: 80 (HTTP), 443 (HTTPS)
   - SSL termination
   - Rate limiting and caching
   - Static file serving

### Port Mapping

| Service | Host Port | Container Port |
|---------|-----------|----------------|
| Frontend | 3000 | 3000 |
| Backend | 8000 | 8000 |

| Nginx (HTTP) | 80 | 80 |
| Nginx (HTTPS) | 443 | 443 |

## Environment Variables

Create a `.env` file in the project root with the following variables:

```env
# Backend
CORS_ORIGINS=http://localhost:3000,https://localhost:3000,http://nginx,https://nginx

# Frontend
NEXT_PUBLIC_API_URL=http://localhost:8000
NEXTAUTH_URL=http://localhost:3000

# Nginx
NGINX_PORT=80
NGINX_SSL_PORT=443
```

## Building and Deployment

### Building Individual Services

```bash
# Build frontend only
docker build -f Dockerfile.frontend -t snapo-frontend .

# Build backend only
docker build -f Dockerfile.backend -t snapo-backend .

# Build and tag for production
docker build -f Dockerfile.frontend -t snapo-frontend:latest .
docker build -f Dockerfile.backend -t snapo-backend:latest .
```

### Pushing to Registry

```bash
# Tag images with registry
docker tag snapo-frontend:latest your-registry.com/snapo-frontend:latest
docker tag snapo-backend:latest your-registry.com/snapo-backend:latest

# Push to registry
docker push your-registry.com/snapo-frontend:latest
docker push your-registry.com/snapo-backend:latest
```

## Development Workflow

### Running in Development Mode

```bash
# Start all services
docker-compose up --build

# Start specific services
docker-compose up --build frontend backend

# View logs for specific service
docker-compose logs -f frontend

# Access running container
docker-compose exec backend /bin/bash
```

### Hot Reloading

The frontend service includes hot reloading. Changes to frontend code will automatically rebuild and restart the frontend container.

### Database Management

The backend uses a local SQLite database named `prodz.db` mapped via volume to the host. You can back it up by directly copying the file.

## Production Deployment

### SSL Certificate Setup

1. Generate SSL certificates:
```bash
# Self-signed certificate (development)
openssl req -x509 -newkey rsa:4096 -keyout ssl/key.pem -out ssl/cert.pem -days 365 -nodes

# Or use Let's Encrypt for production
# (Use certbot or similar tools)
```

2. Mount certificates in nginx:
```yaml
volumes:
  - ./ssl:/etc/nginx/ssl:ro
```

### Scaling

```bash
# Scale backend instances
docker-compose up --scale backend=3 --build

# Scale frontend instances
docker-compose up --scale frontend=2 --build
```

### Monitoring

```bash
# View container stats
docker stats

# View logs for all services
docker-compose logs -f

# View logs for specific service
docker-compose logs -f backend
```

## Troubleshooting

### Common Issues

1. **Port already in use**
```bash
# Stop existing containers
docker-compose down

# Or use different ports in .env file
```

2. **Build failures**
```bash
# Clean up old images and containers
docker system prune -a

# Rebuild with cache cleared
docker-compose build --no-cache
```

### Health Checks

All services include health checks. Verify service health:

```bash
# Check backend health
curl http://localhost:8000/health

# Check nginx health
curl http://localhost/health
```

## Security Considerations

1. **Environment Variables**
   - Never commit sensitive data to version control
   - Use `.env` files for development, environment variables for production

2. **Network Security**
   - Use separate networks for different services
   - Enable SSL/TLS for production
   - Implement rate limiting

3. **Access Control**
   - Use non-root users in containers
   - Implement proper authentication and authorization
   - Regularly update base images

## Performance Optimization

### Memory Usage

- Frontend: ~500MB
- Backend: ~300MB
- Nginx: ~50MB

### Disk Space

- Images: ~2GB
- Volumes: SQLite DB requires minimal space initially

### Optimization Tips

1. Use multi-stage builds
2. Enable gzip compression
3. Implement caching strategies
4. Use connection pooling
5. Monitor resource usage

## Backup and Recovery

### Database Backup

As the database is a simple SQLite file (`prodz.db`) mounted in the project directory, you can back it up by copying the file:
```bash
cp prodz.db prodz_backup_$(date +%Y%m%d_%H%M%S).db
```

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Deploy to Production

on:
  push:
    branches: [ main ]

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Build and push Docker images
        run: |
          docker-compose build
          docker-compose push

      - name: Deploy to server
        uses: appleboy/ssh-action@v1.0.0
        with:
          host: ${{ secrets.HOST }}
          username: ${{ secrets.USERNAME }}
          key: ${{ secrets.SSH_KEY }}
          script: |
            cd /path/to/project
            docker-compose pull
            docker-compose up -d
```
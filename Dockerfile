# Use rust base image
FROM rust:latest

WORKDIR /app

# Copy the current directory contents into the container at /app
COPY . /app

# Build the project
RUN cargo build --release

# Run the compiled binary
CMD ["./scripts/entrypoint.sh"]
#!/usr/bin/env python3
"""
Automerge Change Archive Worker

Subscribes to Redis pub/sub channels for Automerge document changes
and archives them to PostgreSQL for persistence and historical tracking.
"""

import os
import sys
import time
import base64
import logging
from typing import Optional

import redis
import psycopg2
from psycopg2.extras import execute_values

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)


class ChangeArchiveWorker:
    """Worker that subscribes to Redis changes and archives them to PostgreSQL"""

    def __init__(
        self,
        redis_host: str = 'redis',
        redis_port: int = 6379,
        postgres_dsn: str = None
    ):
        self.redis_host = redis_host
        self.redis_port = redis_port
        self.postgres_dsn = postgres_dsn or self._build_postgres_dsn()

        self.redis_client: Optional[redis.Redis] = None
        self.pubsub: Optional[redis.client.PubSub] = None
        self.pg_conn: Optional[psycopg2.extensions.connection] = None

        logger.info(f"Initializing worker - Redis: {redis_host}:{redis_port}")

    def _build_postgres_dsn(self) -> str:
        """Build PostgreSQL connection string from environment variables"""
        return (
            f"host={os.getenv('POSTGRES_HOST', 'postgres')} "
            f"port={os.getenv('POSTGRES_PORT', '5432')} "
            f"dbname={os.getenv('POSTGRES_DB', 'automerge')} "
            f"user={os.getenv('POSTGRES_USER', 'automerge')} "
            f"password={os.getenv('POSTGRES_PASSWORD', 'automerge_dev_password')}"
        )

    def connect_redis(self) -> None:
        """Establish connection to Redis"""
        try:
            self.redis_client = redis.Redis(
                host=self.redis_host,
                port=self.redis_port,
                decode_responses=False  # We need binary data
            )
            # Test connection
            self.redis_client.ping()
            logger.info("Connected to Redis successfully")
        except Exception as e:
            logger.error(f"Failed to connect to Redis: {e}")
            raise

    def connect_postgres(self) -> None:
        """Establish connection to PostgreSQL"""
        max_retries = 10
        retry_delay = 2

        for attempt in range(max_retries):
            try:
                self.pg_conn = psycopg2.connect(self.postgres_dsn)
                self.pg_conn.autocommit = False  # Use transactions
                logger.info("Connected to PostgreSQL successfully")
                return
            except psycopg2.OperationalError as e:
                if attempt < max_retries - 1:
                    logger.warning(
                        f"Failed to connect to PostgreSQL (attempt {attempt + 1}/{max_retries}): {e}"
                    )
                    time.sleep(retry_delay)
                else:
                    logger.error(f"Failed to connect to PostgreSQL after {max_retries} attempts")
                    raise

    def subscribe_to_changes(self) -> None:
        """Subscribe to all Redis pub/sub channels for document changes"""
        self.pubsub = self.redis_client.pubsub()

        # Subscribe to all document change channels
        # Pattern: changes:am:document:*
        self.pubsub.psubscribe('changes:am:document:*')
        logger.info("Subscribed to pattern: changes:am:document:*")

    def extract_document_uuid(self, channel: str) -> Optional[str]:
        """
        Extract document UUID from channel name

        Channel format: changes:am:document:{uuid}
        Returns: uuid string or None if invalid format
        """
        try:
            # Channel format: changes:am:document:{uuid}
            # or in bytes: b'changes:am:document:{uuid}'
            if isinstance(channel, bytes):
                channel = channel.decode('utf-8')

            parts = channel.split(':')
            if len(parts) >= 4 and parts[0] == 'changes' and parts[1] == 'am' and parts[2] == 'document':
                document_uuid = parts[3]
                return document_uuid
            else:
                logger.warning(f"Unexpected channel format: {channel}")
                return None
        except Exception as e:
            logger.error(f"Error extracting document UUID from channel {channel}: {e}")
            return None

    def archive_change(self, document_uuid: str, change_data: bytes) -> bool:
        """
        Archive a single change to PostgreSQL

        Args:
            document_uuid: UUID of the document
            change_data: Binary Automerge change data

        Returns:
            True if successful, False otherwise
        """
        try:
            with self.pg_conn.cursor() as cursor:
                cursor.execute(
                    """
                    INSERT INTO document_changes (document_uuid, change_data)
                    VALUES (%s, %s)
                    """,
                    (document_uuid, psycopg2.Binary(change_data))
                )
            self.pg_conn.commit()
            return True
        except Exception as e:
            logger.error(f"Error archiving change for document {document_uuid}: {e}")
            self.pg_conn.rollback()
            return False

    def process_message(self, message: dict) -> None:
        """
        Process a single pub/sub message

        Args:
            message: Redis pub/sub message dictionary
        """
        if message['type'] == 'pmessage':
            channel = message['channel']
            data = message['data']

            # Extract document UUID from channel name
            document_uuid = self.extract_document_uuid(channel)
            if not document_uuid:
                logger.warning(f"Could not extract document UUID from channel: {channel}")
                return

            # The data is base64-encoded change bytes sent from webdis
            # We need to decode it back to binary
            try:
                if isinstance(data, str):
                    # Data is base64-encoded string
                    change_bytes = base64.b64decode(data)
                elif isinstance(data, bytes):
                    # Data might already be binary or base64-encoded bytes
                    try:
                        # Try to decode as base64
                        change_bytes = base64.b64decode(data)
                    except:
                        # If that fails, assume it's already binary
                        change_bytes = data
                else:
                    logger.error(f"Unexpected data type: {type(data)}")
                    return

                # Archive to PostgreSQL
                success = self.archive_change(document_uuid, change_bytes)
                if success:
                    logger.info(
                        f"Archived change for document {document_uuid} "
                        f"({len(change_bytes)} bytes)"
                    )
                else:
                    logger.error(f"Failed to archive change for document {document_uuid}")

            except Exception as e:
                logger.error(f"Error processing change data: {e}")

    def run(self) -> None:
        """Main worker loop"""
        logger.info("Starting Automerge Change Archive Worker")

        try:
            # Connect to Redis and PostgreSQL
            self.connect_redis()
            self.connect_postgres()

            # Subscribe to change channels
            self.subscribe_to_changes()

            logger.info("Worker is running and listening for changes...")

            # Listen for messages
            for message in self.pubsub.listen():
                self.process_message(message)

        except KeyboardInterrupt:
            logger.info("Received shutdown signal")
        except Exception as e:
            logger.error(f"Worker error: {e}", exc_info=True)
            sys.exit(1)
        finally:
            self.shutdown()

    def shutdown(self) -> None:
        """Clean shutdown of connections"""
        logger.info("Shutting down worker...")

        if self.pubsub:
            try:
                self.pubsub.unsubscribe()
                self.pubsub.close()
            except Exception as e:
                logger.error(f"Error closing Redis pub/sub: {e}")

        if self.redis_client:
            try:
                self.redis_client.close()
            except Exception as e:
                logger.error(f"Error closing Redis connection: {e}")

        if self.pg_conn:
            try:
                self.pg_conn.close()
            except Exception as e:
                logger.error(f"Error closing PostgreSQL connection: {e}")

        logger.info("Worker shutdown complete")


def main():
    """Entry point"""
    worker = ChangeArchiveWorker()
    worker.run()


if __name__ == '__main__':
    main()

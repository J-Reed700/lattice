/**
 * Update Checker API Types
 *
 * Type definitions for update checking functionality.
 * These types match the Rust backend structures from commands/update_checker.rs
 */

export type { UpdateInfoDto, VersionInfoDto } from '@/lib/bindings';

export type UpdateInfo = import('@/lib/bindings').UpdateInfoDto;

/** What `get_version_info` returns. */
export type VersionInfo = import('@/lib/bindings').VersionInfoDto;

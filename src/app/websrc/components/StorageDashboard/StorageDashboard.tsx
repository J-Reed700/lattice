import React, { useState, useEffect } from 'react';

import {
  HardDrive,
  Database,
  Image,
  FileText,
  RefreshCw,
  Folder,
} from 'lucide-react';

import { useInterval } from '../../hooks';
import VaultAPI from '../../lib/api';
import { handleAsyncEvent } from '../../utils/promiseHandlers';

interface StorageBreakdown {
  original_files: number;
  embeddings: number;
  database: number;
  thumbnails: number;
  cache: number;
  logs: number;
}

interface StorageStats {
  total_bytes: number;
  breakdown: StorageBreakdown;
  by_file_type: Record<string, number>;
  by_date: Record<string, number>;
  largest_files: FileInfo[];
  last_calculated: string;
  cached: boolean;
}

interface FileInfo {
  path: string;
  size_bytes: number;
  modified_at: string;
  file_type: string;
  category: string;
}

const formatBytes = (bytes: number): string => {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let size = bytes;
  let unitIndex = 0;

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex++;
  }

  return `${size.toFixed(2)} ${units[unitIndex]}`;
};

const formatPercent = (value: number): string => `${value.toFixed(1)}%`;

const CategoryCard: React.FC<{
  icon: React.ReactNode;
  title: string;
  bytes: number;
  totalBytes: number;
  color: string;
}> = ({ icon, title, bytes, totalBytes, color }) => {
  const percentage = totalBytes > 0 ? (bytes / totalBytes) * 100 : 0;

  return (
    <div className="bg-[hsl(var(--surface-raised))] rounded-lg p-4 shadow-sm border border-[hsl(var(--border-subtle))]">
      <div className="flex items-center justify-between mb-2">
        <div className="flex items-center gap-2">
          <div className={`${color} p-2 rounded-lg`}>
            {icon}
          </div>
          <span className="font-medium text-[hsl(var(--text-primary))]">{title}</span>
        </div>
        <span className="text-sm text-[hsl(var(--text-secondary))]">
          {formatPercent(percentage)}
        </span>
      </div>
      <div className="mt-2">
        <div className="text-lg font-semibold text-[hsl(var(--text-primary))]">
          {formatBytes(bytes)}
        </div>
        <div className="mt-2 h-2 bg-[hsl(var(--surface-raised))] rounded-full overflow-hidden">
          <div
            className={`h-full ${color.replace('bg-', 'bg-opacity-50 bg-')}`}
            style={{ width: `${Math.min(percentage, 100)}%` }}
          />
        </div>
      </div>
    </div>
  );
};

const StorageDashboard: React.FC = () => {
  const [stats, setStats] = useState<StorageStats | null>(null);
  const [loading, setLoading] = useState(true);

  const fetchStorageStats = async (refresh = false) => {
    try {
      setLoading(true);
      const result = await VaultAPI.getSystemStats();
      if (result.ok) {
        setStats({
          total_bytes: result.data.storage_size_bytes,
          breakdown: {
            original_files: 0,
            embeddings: 0,
            database: 0,
            thumbnails: 0,
            cache: 0,
            logs: 0,
          },
          by_file_type: {},
          by_date: {},
          largest_files: [],
          last_calculated: new Date().toISOString(),
          cached: false,
        });
      } else if (!refresh) {
        setStats(null);
      }
    } catch (error) {
      console.error("Couldn't fetch storage stats:", error);
      setStats(null);
    } finally {
      setLoading(false);
    }
  };

  // Load initial data on mount
  useEffect(() => {
    void fetchStorageStats();
  }, []);

  // Poll storage stats every minute
  useInterval(
    () => {
      void fetchStorageStats();
    },
    60000 // 1 minute
  );

  if (loading || !stats) {
    return (
      <div className="flex items-center justify-center h-96">
        <RefreshCw className="w-8 h-8 animate-spin text-[hsl(var(--accent))]" />
      </div>
    );
  }

  return (
    <div className="max-w-7xl mx-auto p-6 space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold text-[hsl(var(--text-primary))]">Storage management</h1>
          <p className="text-[hsl(var(--text-secondary))] mt-1">
            Monitor and manage your Vault storage usage
          </p>
        </div>
        <button
          onClick={handleAsyncEvent(async () => fetchStorageStats(true))}
          className="flex items-center gap-2 px-4 py-2 bg-[hsl(var(--accent))] text-[hsl(var(--accent-fg))] rounded-md hover:bg-[hsl(var(--accent-hover))] transition-colors duration-fast"
        >
          <RefreshCw className="w-4 h-4" strokeWidth={1.75} />
          Refresh
        </button>
      </div>

      {/* Total Storage */}
      <div className="bg-[hsl(var(--accent))] rounded-lg p-6 shadow-sm text-[hsl(var(--accent-fg))]">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-[hsl(var(--accent-fg))]/80 text-sm font-medium mb-1">Total storage used</p>
            <h2 className="text-4xl font-bold">{formatBytes(stats.total_bytes)}</h2>
          </div>
          <HardDrive className="w-12 h-12 opacity-80" strokeWidth={1.75} />
        </div>
      </div>

      {/* Category Breakdown */}
      <div>
        <h2 className="text-xl font-semibold text-[hsl(var(--text-primary))] mb-4">
          Storage by Category
        </h2>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          <CategoryCard
            icon={<FileText className="w-5 h-5 text-[hsl(var(--accent))]" />}
            title="Original Files"
            bytes={stats.breakdown.original_files}
            totalBytes={stats.total_bytes}
            color="bg-[hsl(var(--accent-muted))]"
          />
          <CategoryCard
            icon={<Database className="w-5 h-5 text-[hsl(var(--warning-fg))]" />}
            title="Embeddings"
            bytes={stats.breakdown.embeddings}
            totalBytes={stats.total_bytes}
            color="bg-[hsl(var(--warning-muted))]/20"
          />
          <CategoryCard
            icon={<Database className="w-5 h-5 text-[hsl(var(--success-fg))]" />}
            title="Database"
            bytes={stats.breakdown.database}
            totalBytes={stats.total_bytes}
            color="bg-[hsl(var(--success-muted))]"
          />
          <CategoryCard
            icon={<Image className="w-5 h-5 text-[hsl(var(--danger-fg))]" />}
            title="Thumbnails"
            bytes={stats.breakdown.thumbnails}
            totalBytes={stats.total_bytes}
            color="bg-[hsl(var(--danger-muted))]/20"
          />
          <CategoryCard
            icon={<Folder className="w-5 h-5 text-[hsl(var(--warning-fg))]" />}
            title="Cache"
            bytes={stats.breakdown.cache}
            totalBytes={stats.total_bytes}
            color="bg-[hsl(var(--warning-muted))]"
          />
          <CategoryCard
            icon={<FileText className="w-5 h-5 text-[hsl(var(--text-secondary))]" />}
            title="Logs"
            bytes={stats.breakdown.logs}
            totalBytes={stats.total_bytes}
            color="bg-[hsl(var(--surface-raised))]"
          />
        </div>
      </div>

      {/* Largest Files */}
      {stats.largest_files.length > 0 && (
        <div>
          <h2 className="text-xl font-semibold text-[hsl(var(--text-primary))] mb-4">
            Largest Files
          </h2>
          <div className="bg-[hsl(var(--surface-raised))] rounded-lg shadow-sm border border-[hsl(var(--border-subtle))] overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full">
                <thead className="bg-[hsl(var(--surface))]">
                  <tr>
                    <th className="px-6 py-3 text-left text-xs font-medium text-[hsl(var(--text-secondary))] uppercase tracking-wider">
                      File
                    </th>
                    <th className="px-6 py-3 text-left text-xs font-medium text-[hsl(var(--text-secondary))] uppercase tracking-wider">
                      Type
                    </th>
                    <th className="px-6 py-3 text-left text-xs font-medium text-[hsl(var(--text-secondary))] uppercase tracking-wider">
                      Size
                    </th>
                    <th className="px-6 py-3 text-left text-xs font-medium text-[hsl(var(--text-secondary))] uppercase tracking-wider">
                      Category
                    </th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-[hsl(var(--border-subtle))]">
                  {stats.largest_files.slice(0, 10).map((file, index) => (
                    <tr key={index} className="hover:bg-[hsl(var(--surface))]/50">
                      <td className="px-6 py-4 text-sm text-[hsl(var(--text-primary))] truncate max-w-xs">
                        {file.path.split('/').pop()}
                      </td>
                      <td className="px-6 py-4 text-sm text-[hsl(var(--text-secondary))]">
                        {file.file_type.toUpperCase()}
                      </td>
                      <td className="px-6 py-4 text-sm font-medium text-[hsl(var(--text-primary))]">
                        {formatBytes(file.size_bytes)}
                      </td>
                      <td className="px-6 py-4 text-sm text-[hsl(var(--text-secondary))]">
                        {file.category}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}

    </div>
  );
};

export default StorageDashboard;

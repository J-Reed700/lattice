import { motion } from 'framer-motion';
import { FileText, Database, Search, Clock, FileUp, Globe } from 'lucide-react';
import { useNavigate } from 'react-router-dom';

import { BentoCard } from '@/components/ui/BentoCard';
import { StatCard } from '@/components/ui/StatCard';
import { useDashboardQuery } from '@/hooks/queries';
import { fadeInUp, staggerContainer } from '@/lib/animations';
import type { RecentDocument } from '@/types';
import { formatRelativeTime, formatBytes } from '@/utils/formatters';

import { DashboardError } from './DashboardError';
import { DashboardSkeleton } from './DashboardSkeleton';


interface DashboardProps {
  onNavigate?: (view: 'search' | 'files' | 'settings') => void;
}

export const Dashboard = (_props: DashboardProps) => {
  console.log('[DASHBOARD] Dashboard component mounting...');

  // TEMP: Test render before any hooks
  // return <div style={{ padding: 40, color: 'red', fontSize: 24 }}>DASHBOARD TEST</div>;

  const { data, isLoading, error } = useDashboardQuery();
  console.log('[DASHBOARD] Query state:', { isLoading, error: error?.message, hasData: !!data, data });
  const navigate = useNavigate();

  if (isLoading) {
    console.log('[DASHBOARD] Showing skeleton (loading)');
    return <DashboardSkeleton />;
  }

  if (error) {
    console.log('[DASHBOARD] Showing error:', error.message);
    return <DashboardError error={error.message} />;
  }

  console.log('[DASHBOARD] Rendering main content');

  return (
    <motion.div
      variants={staggerContainer}
      initial="hidden"
      animate="visible"
      className="h-full overflow-auto bg-bg-primary"
      style={{ backgroundImage: 'var(--gradient-mesh)' }}
    >
      <div className="container mx-auto p-6 space-y-8">
        <motion.div variants={fadeInUp} className="space-y-2">
          <h1 className="text-4xl md:text-5xl font-extrabold tracking-display bg-gradient-to-r from-sky-400 via-purple-400 to-sky-400 bg-clip-text text-transparent">
            Welcome back
          </h1>
          <p className="text-xl text-text-secondary">
            Here's your knowledge hub
          </p>
        </motion.div>

        {/* Quick Actions */}
        <motion.div variants={fadeInUp} className="space-y-3">
          <h2 className="text-xl font-semibold">Quick Actions</h2>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <button
              onClick={() => navigate('/ingest')}
              className="p-4 bg-gradient-to-br from-[var(--accent-primary)]/5 to-[var(--accent-primary)]/10 border border-[var(--accent-primary)]/20 rounded-xl hover:border-[var(--accent-primary)]/40 hover:shadow-lg transition-all duration-200"
            >
              <div className="flex items-center gap-3">
                <div className="p-2.5 bg-[var(--accent-primary)]/15 rounded-xl group-hover:bg-[var(--accent-primary)]/20 transition-colors">
                  <FileUp className="w-5 h-5 text-[var(--accent-primary)]" />
                </div>
                <div className="text-left">
                  <div className="font-semibold">Add Files</div>
                  <div className="text-sm text-text-secondary">Import local files</div>
                </div>
              </div>
            </button>

            <button
              onClick={() => navigate('/ingest')}
              className="p-4 bg-gradient-to-br from-[var(--success)]/5 to-[var(--success)]/10 border border-[var(--success)]/20 rounded-xl hover:border-[var(--success)]/40 hover:shadow-lg transition-all duration-200"
            >
              <div className="flex items-center gap-3">
                <div className="p-2.5 bg-[var(--success)]/15 rounded-xl transition-colors">
                  <Globe className="w-5 h-5 text-[var(--success)]" />
                </div>
                <div className="text-left">
                  <div className="font-semibold">Add Web Page</div>
                  <div className="text-sm text-text-secondary">Import from URL</div>
                </div>
              </div>
            </button>

            <button
              onClick={() => navigate('/search')}
              className="p-4 bg-gradient-to-br from-[var(--warning)]/5 to-[var(--warning)]/10 border border-[var(--warning)]/20 rounded-xl hover:border-[var(--warning)]/40 hover:shadow-lg transition-all duration-200"
            >
              <div className="flex items-center gap-3">
                <div className="p-2.5 bg-[var(--warning)]/15 rounded-xl transition-colors">
                  <Search className="w-5 h-5 text-[var(--warning)]" />
                </div>
                <div className="text-left">
                  <div className="font-semibold">Search</div>
                  <div className="text-sm text-text-secondary">Find in knowledge base</div>
                </div>
              </div>
            </button>
          </div>
        </motion.div>

        <motion.div
          variants={fadeInUp}
          className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 auto-rows-fr"
        >
          <BentoCard className="md:col-span-2 lg:row-span-2 bg-gradient-to-br from-[var(--accent-primary)]/10 to-[var(--accent-primary)]/10 dark:from-[var(--accent-primary)]/20 dark:to-[var(--accent-primary)]/20">
            <div className="h-full flex flex-col">
              <h2 className="text-2xl font-bold mb-4">Recent Activity</h2>
              {data?.recentDocuments && data.recentDocuments.length > 0 ? (
                <div className="space-y-2 flex-1 overflow-y-auto">
                  {data.recentDocuments.slice(0, 5).map((doc: RecentDocument, index: number) => (
                    <motion.div
                      key={doc.id || index}
                      initial={{ opacity: 0, x: -20 }}
                      animate={{ opacity: 1, x: 0 }}
                      transition={{ delay: index * 0.1 }}
                      className="p-3 rounded-md bg-bg-primary/50 hover:bg-bg-primary transition-colors cursor-pointer"
                    >
                      <div className="flex items-start gap-2">
                        <FileText className="w-4 h-4 mt-1 text-accent-primary flex-shrink-0" />
                        <div className="flex-1 min-w-0">
                          <div className="font-medium text-sm truncate">
                            {doc.fileName || doc.filePath || 'Untitled'}
                          </div>
                          <div className="text-xs text-text-secondary">
                            {formatRelativeTime(doc.modifiedAt || doc.indexedAt, '')}
                          </div>
                        </div>
                      </div>
                    </motion.div>
                  ))}
                </div>
              ) : (
                <p className="text-text-secondary">No recent activity</p>
              )}
            </div>
          </BentoCard>

          <BentoCard className="p-4 flex flex-col items-center justify-center text-center bg-gradient-to-br from-[var(--success)]/10 to-[var(--success)]/10 dark:from-[var(--success)]/20 dark:to-[var(--success)]/20">
            <StatCard
              icon={<FileText className="w-8 h-8" />}
              value={data?.stats?.documentCount ?? 0}
              label="Documents"
              trend={(data?.stats?.documentCount ?? 0) > 0 ? "+12%" : undefined}
              trendDirection="up"
            />
          </BentoCard>

          <BentoCard className="p-4 flex flex-col items-center justify-center text-center bg-gradient-to-br from-[var(--accent-primary)]/10 to-[var(--accent-primary)]/10 dark:from-[var(--accent-primary)]/20 dark:to-[var(--accent-primary)]/20">
            <StatCard
              icon={<Database className="w-8 h-8" />}
              value={formatBytes(data?.stats?.storageUsed || 0)}
              label="Storage Used"
            />
          </BentoCard>

          <BentoCard className="p-4 flex flex-col items-center justify-center text-center">
            <StatCard
              icon={<Search className="w-8 h-8" />}
              value={data?.stats?.searchCount || 0}
              label="Searches Today"
            />
          </BentoCard>

          <BentoCard className="p-4 flex flex-col items-center justify-center text-center">
            <StatCard
              icon={<Clock className="w-8 h-8" />}
              value={formatRelativeTime(data?.stats?.lastIndexed, 'Never')}
              label="Last Indexed"
            />
          </BentoCard>
        </motion.div>
      </div>
    </motion.div>
  );
};

export default Dashboard;

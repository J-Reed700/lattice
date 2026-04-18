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
  // TEMP: Test render before any hooks
  // return <div style={{ padding: 40, color: 'red', fontSize: 24 }}>DASHBOARD TEST</div>;

  const { data, isLoading, error } = useDashboardQuery();
  const navigate = useNavigate();

  if (isLoading) {
    return <DashboardSkeleton />;
  }

  if (error) {
    return <DashboardError error={error.message} />;
  }

  return (
    <motion.div
      variants={staggerContainer}
      initial="hidden"
      animate="visible"
      className="h-full overflow-auto bg-[hsl(var(--bg))]">
      <div className="container mx-auto p-6 space-y-8">
        <motion.div variants={fadeInUp} className="space-y-2">
          <h1 className="text-4xl md:text-5xl font-extrabold tracking-tight bg-clip-text text-transparent">
            Welcome back
          </h1>
          <p className="text-xl text-[hsl(var(--text-secondary))]">
            Here's your knowledge hub
          </p>
        </motion.div>

        {/* Quick Actions */}
        <motion.div variants={fadeInUp} className="space-y-3">
          <h2 className="text-xl font-semibold">Quick Actions</h2>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <button
              onClick={() => navigate('/ingest')}
              className="p-4 border border-[hsl(var(--accent))]/20 rounded-xl hover:border-[hsl(var(--accent))]/40 hover:shadow-md transition-colors duration-200"
            >
              <div className="flex items-center gap-3">
                <div className="p-2.5 bg-[hsl(var(--accent))]/15 rounded-xl group-hover:bg-[hsl(var(--accent))]/20 transition-colors duration-fast">
                  <FileUp className="w-5 h-5 text-[hsl(var(--accent))]" />
                </div>
                <div className="text-left">
                  <div className="font-semibold">Add Files</div>
                  <div className="text-sm text-[hsl(var(--text-secondary))]">Import local files</div>
                </div>
              </div>
            </button>

            <button
              onClick={() => navigate('/ingest')}
              className="p-4 border border-[hsl(var(--success-fg))]/20 rounded-xl hover:border-[hsl(var(--success-fg))]/40 hover:shadow-md transition-colors duration-200"
            >
              <div className="flex items-center gap-3">
                <div className="p-2.5 bg-[hsl(var(--success-fg))]/15 rounded-xl transition-colors duration-fast">
                  <Globe className="w-5 h-5 text-[hsl(var(--success-fg))]" />
                </div>
                <div className="text-left">
                  <div className="font-semibold">Add Web Page</div>
                  <div className="text-sm text-[hsl(var(--text-secondary))]">Import from URL</div>
                </div>
              </div>
            </button>

            <button
              onClick={() => navigate('/search')}
              className="p-4 border border-[hsl(var(--warning-fg))]/20 rounded-xl hover:border-[hsl(var(--warning-fg))]/40 hover:shadow-md transition-colors duration-200"
            >
              <div className="flex items-center gap-3">
                <div className="p-2.5 bg-[hsl(var(--warning-fg))]/15 rounded-xl transition-colors duration-fast">
                  <Search className="w-5 h-5 text-[hsl(var(--warning-fg))]" />
                </div>
                <div className="text-left">
                  <div className="font-semibold">Search</div>
                  <div className="text-sm text-[hsl(var(--text-secondary))]">Find in knowledge base</div>
                </div>
              </div>
            </button>
          </div>
        </motion.div>

        <motion.div
          variants={fadeInUp}
          className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 auto-rows-fr"
        >
          <BentoCard className="md:col-span-2 lg:row-span-2">
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
                      className="p-3 rounded-md bg-[hsl(var(--bg))]/50 hover:bg-[hsl(var(--bg))] transition-colors duration-fast cursor-pointer"
                    >
                      <div className="flex items-start gap-2">
                        <FileText className="w-4 h-4 mt-1 text-[hsl(var(--accent))] flex-shrink-0" />
                        <div className="flex-1 min-w-0">
                          <div className="font-medium text-sm truncate">
                            {doc.fileName || doc.filePath || 'Untitled'}
                          </div>
                          <div className="text-xs text-[hsl(var(--text-secondary))]">
                            {formatRelativeTime(doc.modifiedAt || doc.indexedAt, '')}
                          </div>
                        </div>
                      </div>
                    </motion.div>
                  ))}
                </div>
              ) : (
                <p className="text-[hsl(var(--text-secondary))]">No recent activity</p>
              )}
            </div>
          </BentoCard>

          <BentoCard className="p-4 flex flex-col items-center justify-center text-center">
            <StatCard
              icon={<FileText className="w-8 h-8" />}
              value={data?.stats?.documentCount ?? 0}
              label="Documents"
              trend={(data?.stats?.documentCount ?? 0) > 0 ? "+12%" : undefined}
              trendDirection="up"
            />
          </BentoCard>

          <BentoCard className="p-4 flex flex-col items-center justify-center text-center">
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

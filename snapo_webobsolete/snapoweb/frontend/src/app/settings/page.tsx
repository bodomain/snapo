"use client";

import MergeDB from "@/components/MergeDB";

export default function SettingsPage() {
  return (
    <div className="space-y-8 animate-in fade-in zoom-in-95 duration-500">
      <header className="mb-8">
        <h2 className="text-3xl font-bold tracking-tight text-white mb-2">Settings</h2>
        <p className="text-zinc-400">Manage your application data and preferences.</p>
      </header>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-zinc-900/60 backdrop-blur-md border border-zinc-800 rounded-3xl p-6 md:p-8 shadow-2xl shadow-black/50">
          <h3 className="text-xl font-semibold mb-6 flex items-center gap-2">
            <span className="w-1.5 h-6 rounded-full bg-purple-500 inline-block" />
            Data Management
          </h3>
          <MergeDB onMergeSuccess={() => {}} />
        </div>
      </div>
    </div>
  );
}

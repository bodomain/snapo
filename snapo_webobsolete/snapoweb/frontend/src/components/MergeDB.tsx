"use client";

import { useState } from "react";
import axios from "axios";
import { UploadCloud, CheckCircle2, AlertCircle, Loader2 } from "lucide-react";

export default function MergeDB({ onMergeSuccess }: { onMergeSuccess: () => void }) {
  const [file, setFile] = useState<File | null>(null);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<{ text: string; isError: boolean } | null>(null);

  const API_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8000";

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files) {
      setFile(e.target.files[0]);
      setMessage(null);
    }
  };

  const handleMerge = async () => {
    if (!file) return;

    const formData = new FormData();
    formData.append("file", file);

    setLoading(true);
    setMessage(null);

    try {
      const response = await axios.post(`${API_URL}/api/database/merge`, formData, {
        headers: { "Content-Type": "multipart/form-data" },
      });
      setMessage({ text: response.data.message || "Merge successful", isError: false });
      setFile(null); // reset file input
      onMergeSuccess();
    } catch (err: any) {
      console.error(err);
      setMessage({ 
        text: err.response?.data?.detail || "An error occurred during merge", 
        isError: true 
      });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex flex-col gap-5">
      <div className="flex flex-col gap-2">
        <p className="text-sm text-zinc-400">
          Upload a backup <code>.db</code> file from another device to merge its sessions into your current database.
        </p>
      </div>
      
      <div className="flex flex-col md:flex-row gap-4">
        <div className="relative flex-grow h-14 group">
          <input 
            type="file" 
            accept=".db" 
            onChange={handleFileChange}
            className="absolute inset-0 w-full h-full opacity-0 cursor-pointer z-10"
          />
          <div className={`w-full h-full border-2 border-dashed rounded-xl flex items-center px-4 gap-3 transition-colors ${
            file 
              ? "border-indigo-500/50 bg-indigo-500/10 text-indigo-300" 
              : "border-zinc-700 bg-zinc-900/50 text-zinc-400 group-hover:border-zinc-600 group-hover:bg-zinc-800/80"
          }`}>
            <UploadCloud className="w-5 h-5" />
            <span className="truncate font-medium">
              {file ? file.name : "Choose .db file..."}
            </span>
          </div>
        </div>

        <button 
          onClick={handleMerge}
          disabled={!file || loading}
          className="h-14 px-8 rounded-xl font-bold transition-all disabled:opacity-50 disabled:cursor-not-allowed bg-purple-600 hover:bg-purple-500 text-white shadow-lg shadow-purple-600/20 flex items-center justify-center gap-2 min-w-[140px]"
        >
          {loading ? (
             <><Loader2 className="w-5 h-5 animate-spin"/> Merging...</>
          ) : (
            "Merge Data"
          )}
        </button>
      </div>

      {message && (
        <div className={`flex items-center gap-2 p-3 mt-2 rounded-lg text-sm border ${
          message.isError 
            ? "bg-red-950/50 border-red-500/20 text-red-400 font-medium" 
            : "bg-emerald-950/50 border-emerald-500/20 text-emerald-400 font-medium"
        }`}>
          {message.isError ? <AlertCircle className="w-4 h-4 shrink-0"/> : <CheckCircle2 className="w-4 h-4 shrink-0"/>}
          <span>{message.text}</span>
        </div>
      )}
    </div>
  );
}

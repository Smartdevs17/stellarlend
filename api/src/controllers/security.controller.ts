import { Request, Response } from 'express';
import { vulnerabilityDisclosureService } from '../services/vulnerability-disclosure';
import { assetShutdownService } from '../services/asset-shutdown';

export const submitVulnerabilityReport = async (req: Request, res: Response) => {
  try {
    const report = await vulnerabilityDisclosureService.submitReport(req.body);
    res.status(201).json({ success: true, data: report });
  } catch (error: any) {
    res.status(400).json({ success: false, message: error.message });
  }
};

export const getVulnerabilityReport = async (req: Request, res: Response) => {
  try {
    const report = await vulnerabilityDisclosureService.getReportById(req.params.id);
    if (!report) {
      return res.status(404).json({ success: false, message: 'Report not found' });
    }
    res.json({ success: true, data: report });
  } catch (error: any) {
    res.status(400).json({ success: false, message: error.message });
  }
};

export const getTriageQueue = async (req: Request, res: Response) => {
  try {
    const queue = await vulnerabilityDisclosureService.getTriageQueue();
    res.json({ success: true, data: queue });
  } catch (error: any) {
    res.status(400).json({ success: false, message: error.message });
  }
};

export const shutdownAsset = async (req: Request, res: Response) => {
  try {
    const assetId = req.params.assetId;
    const result = await assetShutdownService.shutdownAsset(assetId, req.body ?? {});
    res.status(200).json({ success: true, data: result });
  } catch (error: any) {
    const status = error.statusCode ?? 400;
    res.status(status).json({ success: false, message: error.message });
  }
};

export const restoreAsset = async (req: Request, res: Response) => {
  try {
    const assetId = req.params.assetId;
    const result = await assetShutdownService.restoreAsset(assetId, req.body ?? {});
    res.status(200).json({ success: true, data: result });
  } catch (error: any) {
    const status = error.statusCode ?? 400;
    res.status(status).json({ success: false, message: error.message });
  }
};

export const getAssetShutdownStatus = async (req: Request, res: Response) => {
  try {
    const assetId = req.params.assetId;
    const status = await assetShutdownService.getShutdownStatus(assetId);
    res.json({ success: true, data: status });
  } catch (error: any) {
    const status = error.statusCode ?? 400;
    res.status(status).json({ success: false, message: error.message });
  }
};

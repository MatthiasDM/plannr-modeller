import type { Tool } from './base';
import { LineTool, RectangleTool, CircleTool, PolygonTool } from './draw';
import { SelectTool, EraserTool, MoveTool, RotateTool, PushPullTool, OffsetTool, PaintTool, TapeTool, TextTool, DimensionTool } from './edit';
import { ArcTool, ThreePointArcTool, TwoPointArcTool, RotatedRectangleTool, ProtractorTool, ZoomWindowTool } from './arc';
import { ScaleTool, FollowMeTool, FlipTool, PlaceComponentTool } from './advanced';
import { OrbitTool, PanTool, ZoomTool, SectionTool } from './camera';

export const placeComponentTool = new PlaceComponentTool();
export const BUILTIN_TOOLS: Tool[] = [
  new SelectTool(), new EraserTool(),
  new LineTool(), new RectangleTool(), new RotatedRectangleTool(), new CircleTool(), new PolygonTool(),
  new ArcTool(false), new ThreePointArcTool(), new TwoPointArcTool(), new ArcTool(true),
  new PushPullTool(), new OffsetTool(), new MoveTool(), new RotateTool(), new ScaleTool(), new FollowMeTool(), new FlipTool(),
  new TapeTool(), new DimensionTool(), new ProtractorTool(), new TextTool(), new PaintTool(), placeComponentTool,
  new OrbitTool(), new PanTool(), new ZoomTool(), new ZoomWindowTool(), new SectionTool(),
];
/** Toolbar layout: groups of tool ids, SketchUp "Large Tool Set" order. */
export const TOOLBAR: string[][] = [
  ['select', 'eraser'],
  ['line', 'rectangle', 'rotrect', 'circle', 'polygon', 'arc', 'arc3', 'arc2', 'pie'],
  ['pushpull', 'offset', 'move', 'rotate', 'scale', 'followme', 'flip', 'place'],
  ['tape', 'dimension', 'protractor', 'text', 'paint'],
  ['orbit', 'pan', 'zoom', 'zoomwin', 'zoomext'],
  ['section'],
];

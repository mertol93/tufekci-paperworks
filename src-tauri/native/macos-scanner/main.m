#import <Foundation/Foundation.h>
#import <ImageCaptureCore/ImageCaptureCore.h>

static const NSUInteger TPProtocolVersion = 1;
static const NSTimeInterval TPDiscoveryBrowseDelay = 3.0;
static const NSTimeInterval TPDiscoveryFallbackDelay = 8.0;
static const NSTimeInterval TPDiscoveryDeadline = 45.0;
static const NSTimeInterval TPCaptureDeadline = 30.0 * 60.0;

typedef NS_ENUM(NSInteger, TPOperation) {
  TPOperationDiscover,
  TPOperationCapture,
};

typedef NS_ENUM(NSInteger, TPPhase) {
  TPPhaseBrowsing,
  TPPhaseDiscoveryOpening,
  TPPhaseDiscoverySelecting,
  TPPhaseDiscoveryClosing,
  TPPhaseCaptureOpening,
  TPPhaseCaptureSelecting,
  TPPhaseCaptureScanning,
  TPPhaseCaptureClosing,
  TPPhaseFinished,
};

static NSString *TPDeviceIdentifier(ICScannerDevice *scanner) {
  NSString *rawIdentifier = scanner.persistentIDString;
  if (rawIdentifier.length == 0) {
    rawIdentifier = scanner.UUIDString;
  }
  if (rawIdentifier.length == 0) {
    rawIdentifier = scanner.serialNumberString;
  }
  if (rawIdentifier.length == 0) {
    rawIdentifier = scanner.name;
  }
  if (rawIdentifier.length == 0) {
    return nil;
  }
  return [@"imagecapture:" stringByAppendingString:rawIdentifier];
}

static BOOL TPFunctionalUnitAvailable(ICScannerDevice *scanner,
                                      ICScannerFunctionalUnitType type) {
  for (NSNumber *value in scanner.availableFunctionalUnitTypes) {
    if (value.unsignedIntegerValue == type) {
      return YES;
    }
  }
  return NO;
}

static NSString *TPErrorDescription(NSError *error, NSString *fallback) {
  if (error.localizedDescription.length > 0) {
    return error.localizedDescription;
  }
  return fallback;
}

@interface TPScannerBridge : NSObject <ICDeviceBrowserDelegate, ICScannerDeviceDelegate>

@property(nonatomic, readonly) BOOL finished;
@property(nonatomic, readonly) int exitCode;

- (instancetype)initWithOperation:(TPOperation)operation request:(NSDictionary *)request;
- (void)start;

@end

@interface TPScannerBridge ()

@property(nonatomic) TPOperation operation;
@property(nonatomic) TPPhase phase;
@property(nonatomic, strong) NSDictionary *request;
@property(nonatomic, strong) ICDeviceBrowser *browser;
@property(nonatomic, strong) NSMutableArray<ICScannerDevice *> *foundScanners;
@property(nonatomic, strong) NSMutableArray<NSMutableDictionary *> *deviceRecords;
@property(nonatomic, strong) ICScannerDevice *activeScanner;
@property(nonatomic) NSUInteger discoveryIndex;
@property(nonatomic, strong) NSMutableDictionary *currentRecord;
@property(nonatomic, strong) NSMutableArray<NSNumber *> *pendingFunctionalUnits;
@property(nonatomic, strong) NSMutableIndexSet *currentResolutions;
@property(nonatomic, strong) NSMutableArray<NSString *> *currentDetailParts;
@property(nonatomic, strong) NSMutableArray<NSString *> *capturePaths;
@property(nonatomic, strong) NSMutableArray<NSString *> *warnings;
@property(nonatomic, strong) NSTimer *settleTimer;
@property(nonatomic, strong) NSTimer *deadlineTimer;
@property(nonatomic) BOOL cancelledAtPageLimit;
@property(nonatomic, readwrite) BOOL finished;
@property(nonatomic, readwrite) int exitCode;

@end

@implementation TPScannerBridge

- (instancetype)initWithOperation:(TPOperation)operation request:(NSDictionary *)request {
  self = [super init];
  if (self != nil) {
    _operation = operation;
    _request = request;
    _phase = TPPhaseBrowsing;
    _foundScanners = [NSMutableArray array];
    _deviceRecords = [NSMutableArray array];
    _pendingFunctionalUnits = [NSMutableArray array];
    _currentResolutions = [NSMutableIndexSet indexSet];
    _currentDetailParts = [NSMutableArray array];
    _capturePaths = [NSMutableArray array];
    _warnings = [NSMutableArray array];
    _exitCode = 1;
  }
  return self;
}

- (void)start {
  self.browser = [[ICDeviceBrowser alloc] init];
  self.browser.delegate = self;
  self.browser.browsedDeviceTypeMask =
      (ICDeviceTypeMask)(ICDeviceTypeMaskScanner | ICDeviceLocationTypeMaskLocal |
                         ICDeviceLocationTypeMaskRemote);
  [self.browser start];

  NSTimeInterval deadline =
      self.operation == TPOperationDiscover ? TPDiscoveryDeadline : TPCaptureDeadline;
  self.deadlineTimer = [NSTimer scheduledTimerWithTimeInterval:deadline
                                                       target:self
                                                     selector:@selector(operationTimedOut:)
                                                     userInfo:nil
                                                      repeats:NO];
  NSTimeInterval fallback = self.operation == TPOperationDiscover
                                ? TPDiscoveryFallbackDelay
                                : TPDiscoveryFallbackDelay + TPDiscoveryBrowseDelay;
  self.settleTimer = [NSTimer scheduledTimerWithTimeInterval:fallback
                                                      target:self
                                                    selector:@selector(browsingSettled:)
                                                    userInfo:nil
                                                     repeats:NO];
}

- (void)operationTimedOut:(NSTimer *)timer {
  (void)timer;
  if (self.operation == TPOperationCapture && self.phase == TPPhaseCaptureScanning) {
    [self.activeScanner cancelScan];
    [self fail:@"Image Capture did not finish the scan before the 30-minute safety limit."];
  } else {
    [self fail:@"Image Capture did not respond before the scanner safety limit."];
  }
}

- (void)browsingSettled:(NSTimer *)timer {
  (void)timer;
  if (self.phase != TPPhaseBrowsing) {
    return;
  }
  if (self.operation == TPOperationDiscover) {
    [self beginDiscoveryInspection];
  } else {
    [self fail:@"The selected scanner is no longer available through Image Capture."];
  }
}

- (void)restartDiscoverySettleTimer {
  [self.settleTimer invalidate];
  self.settleTimer = [NSTimer scheduledTimerWithTimeInterval:TPDiscoveryBrowseDelay
                                                      target:self
                                                    selector:@selector(browsingSettled:)
                                                    userInfo:nil
                                                     repeats:NO];
}

- (void)beginDiscoveryInspection {
  [self.settleTimer invalidate];
  [self.browser stop];
  self.browser.delegate = nil;
  self.browser = nil;
  self.discoveryIndex = 0;
  if (self.foundScanners.count == 0) {
    [self emitDiscovery];
    return;
  }
  [self inspectNextScanner];
}

- (NSMutableDictionary *)baseRecordForScanner:(ICScannerDevice *)scanner {
  NSString *identifier = TPDeviceIdentifier(scanner) ?: @"";
  NSString *name = scanner.name.length > 0 ? scanner.name : @"Image Capture scanner";
  NSString *model = scanner.productKind.length > 0 ? scanner.productKind : name;
  BOOL flatbed = TPFunctionalUnitAvailable(scanner, ICScannerFunctionalUnitTypeFlatbed);
  BOOL feeder =
      TPFunctionalUnitAvailable(scanner, ICScannerFunctionalUnitTypeDocumentFeeder);
  NSMutableDictionary *record = [@{
    @"id" : identifier,
    @"name" : name,
    @"manufacturer" : @"",
    @"model" : model,
    @"flatbed" : @(flatbed),
    @"feeder" : @(feeder),
    @"duplex" : @NO,
    @"supportedDpi" : @[ @150, @300, @600 ],
    @"colourModes" : @[ @"colour", @"greyscale", @"monochrome" ]
  } mutableCopy];

  NSMutableArray<NSString *> *details = [NSMutableArray array];
  if (scanner.transportType.length > 0) {
    [details addObject:scanner.transportType];
  }
  if (scanner.locationDescription.length > 0) {
    [details addObject:scanner.locationDescription];
  }
  if (details.count > 0) {
    record[@"detail"] = [details componentsJoinedByString:@"; "];
  }
  return record;
}

- (void)inspectNextScanner {
  if (self.discoveryIndex >= self.foundScanners.count) {
    [self emitDiscovery];
    return;
  }
  self.activeScanner = self.foundScanners[self.discoveryIndex];
  self.currentRecord = [self baseRecordForScanner:self.activeScanner];
  self.currentResolutions = [NSMutableIndexSet indexSet];
  self.currentDetailParts = [NSMutableArray array];
  self.pendingFunctionalUnits = [NSMutableArray array];
  self.activeScanner.delegate = self;
  self.phase = TPPhaseDiscoveryOpening;
  [self.activeScanner requestOpenSession];
}

- (void)prepareDiscoveryFunctionalUnits {
  for (NSNumber *value in self.activeScanner.availableFunctionalUnitTypes) {
    ICScannerFunctionalUnitType type =
        (ICScannerFunctionalUnitType)value.unsignedIntegerValue;
    if (type == ICScannerFunctionalUnitTypeFlatbed ||
        type == ICScannerFunctionalUnitTypeDocumentFeeder) {
      if (![self.pendingFunctionalUnits containsObject:value]) {
        [self.pendingFunctionalUnits addObject:value];
      }
    }
  }
  ICScannerFunctionalUnit *selected = self.activeScanner.selectedFunctionalUnit;
  if (self.pendingFunctionalUnits.count == 0 && selected != nil &&
      (selected.type == ICScannerFunctionalUnitTypeFlatbed ||
       selected.type == ICScannerFunctionalUnitTypeDocumentFeeder)) {
    [self.pendingFunctionalUnits addObject:@(selected.type)];
  }
  self.currentRecord[@"flatbed"] = @([self.pendingFunctionalUnits
      containsObject:@(ICScannerFunctionalUnitTypeFlatbed)]);
  self.currentRecord[@"feeder"] = @([self.pendingFunctionalUnits
      containsObject:@(ICScannerFunctionalUnitTypeDocumentFeeder)]);
}

- (void)selectNextDiscoveryFunctionalUnit {
  if (self.pendingFunctionalUnits.count == 0) {
    [self finishCurrentDiscoveryRecord];
    return;
  }
  NSNumber *next = self.pendingFunctionalUnits.firstObject;
  [self.pendingFunctionalUnits removeObjectAtIndex:0];
  self.phase = TPPhaseDiscoverySelecting;
  [self.activeScanner
      requestSelectFunctionalUnit:(ICScannerFunctionalUnitType)next.unsignedIntegerValue];
}

- (void)inspectFunctionalUnit:(ICScannerFunctionalUnit *)functionalUnit {
  NSIndexSet *resolutions = functionalUnit.preferredResolutions;
  if (resolutions.count == 0) {
    resolutions = functionalUnit.supportedResolutions;
  }
  NSUInteger value = resolutions.firstIndex;
  NSUInteger added = 0;
  while (value != NSNotFound && added < 12) {
    if (value >= 75 && value <= 1200) {
      [self.currentResolutions addIndex:value];
      added += 1;
    }
    value = [resolutions indexGreaterThanIndex:value];
  }
  if (functionalUnit.type == ICScannerFunctionalUnitTypeDocumentFeeder &&
      [functionalUnit isKindOfClass:[ICScannerFunctionalUnitDocumentFeeder class]]) {
    ICScannerFunctionalUnitDocumentFeeder *feeder =
        (ICScannerFunctionalUnitDocumentFeeder *)functionalUnit;
    self.currentRecord[@"duplex"] = @(feeder.supportsDuplexScanning);
  }
}

- (void)finishCurrentDiscoveryRecord {
  if (self.currentResolutions.count > 0) {
    NSMutableArray<NSNumber *> *resolutions = [NSMutableArray array];
    NSUInteger value = self.currentResolutions.firstIndex;
    while (value != NSNotFound && resolutions.count < 12) {
      [resolutions addObject:@(value)];
      value = [self.currentResolutions indexGreaterThanIndex:value];
    }
    self.currentRecord[@"supportedDpi"] = resolutions;
  }
  if (self.currentDetailParts.count > 0) {
    NSString *existing = self.currentRecord[@"detail"];
    NSString *additional = [self.currentDetailParts componentsJoinedByString:@"; "];
    self.currentRecord[@"detail"] =
        existing.length > 0 ? [NSString stringWithFormat:@"%@; %@", existing, additional]
                            : additional;
  }
  [self.deviceRecords addObject:self.currentRecord];
  if (self.activeScanner.hasOpenSession) {
    self.phase = TPPhaseDiscoveryClosing;
    [self.activeScanner requestCloseSession];
  } else {
    [self moveToNextDiscoveryScanner];
  }
}

- (void)moveToNextDiscoveryScanner {
  self.activeScanner.delegate = nil;
  self.activeScanner = nil;
  self.discoveryIndex += 1;
  [self inspectNextScanner];
}

- (void)openCaptureScanner:(ICScannerDevice *)scanner {
  [self.settleTimer invalidate];
  [self.browser stop];
  self.browser.delegate = nil;
  self.browser = nil;
  self.activeScanner = scanner;
  self.activeScanner.delegate = self;
  self.phase = TPPhaseCaptureOpening;
  [self.activeScanner requestOpenSession];
}

- (void)selectCaptureFunctionalUnit {
  NSString *source = self.request[@"source"];
  ICScannerFunctionalUnitType type = [source isEqualToString:@"feeder"]
                                         ? ICScannerFunctionalUnitTypeDocumentFeeder
                                         : ICScannerFunctionalUnitTypeFlatbed;
  if (!TPFunctionalUnitAvailable(self.activeScanner, type)) {
    [self fail:[source isEqualToString:@"feeder"]
                   ? @"The selected scanner has no document feeder."
                   : @"The selected scanner has no flatbed source."];
    return;
  }
  self.phase = TPPhaseCaptureSelecting;
  [self.activeScanner requestSelectFunctionalUnit:type];
}

- (NSUInteger)nearestResolution:(NSUInteger)requested
                     supported:(NSIndexSet *)supported {
  if (supported.count == 0 || [supported containsIndex:requested]) {
    return requested;
  }
  NSUInteger nearest = supported.firstIndex;
  NSUInteger nearestDistance = NSUIntegerMax;
  NSUInteger value = supported.firstIndex;
  while (value != NSNotFound) {
    NSUInteger distance = value > requested ? value - requested : requested - value;
    if (distance < nearestDistance) {
      nearest = value;
      nearestDistance = distance;
    }
    value = [supported indexGreaterThanIndex:value];
  }
  return nearest == NSNotFound ? requested : nearest;
}

- (CGFloat)measurementValueForMillimetres:(double)millimetres
                                      unit:(ICScannerMeasurementUnit)unit
                                       dpi:(NSUInteger)dpi {
  double inches = millimetres / 25.4;
  switch (unit) {
  case ICScannerMeasurementUnitCentimeters:
    return millimetres / 10.0;
  case ICScannerMeasurementUnitPicas:
    return inches * 6.0;
  case ICScannerMeasurementUnitPoints:
    return inches * 72.0;
  case ICScannerMeasurementUnitTwips:
    return inches * 1440.0;
  case ICScannerMeasurementUnitPixels:
    return inches * (double)dpi;
  case ICScannerMeasurementUnitInches:
  default:
    return inches;
  }
}

- (BOOL)configureFunctionalUnit:(ICScannerFunctionalUnit *)functionalUnit
                           error:(NSString **)errorMessage {
  NSUInteger requestedDpi = [self.request[@"dpi"] unsignedIntegerValue];
  NSUInteger selectedDpi = [self nearestResolution:requestedDpi
                                         supported:functionalUnit.supportedResolutions];
  if (selectedDpi != requestedDpi) {
    [self.warnings
        addObject:[NSString stringWithFormat:
                                @"The scanner used %lu DPI because %lu DPI was unavailable.",
                                (unsigned long)selectedDpi, (unsigned long)requestedDpi]];
  }

  NSString *colourMode = self.request[@"colourMode"];
  ICScannerPixelDataType pixelType = ICScannerPixelDataTypeRGB;
  ICScannerBitDepth bitDepth = ICScannerBitDepth8Bits;
  if ([colourMode isEqualToString:@"greyscale"]) {
    pixelType = ICScannerPixelDataTypeGray;
  } else if ([colourMode isEqualToString:@"monochrome"]) {
    pixelType = ICScannerPixelDataTypeBW;
    bitDepth = ICScannerBitDepth1Bit;
  }

  @try {
    functionalUnit.resolution = selectedDpi;
    functionalUnit.pixelDataType = pixelType;
    if (functionalUnit.supportedBitDepths.count == 0 ||
        [functionalUnit.supportedBitDepths containsIndex:bitDepth]) {
      functionalUnit.bitDepth = bitDepth;
    } else {
      [self.warnings addObject:@"The scanner kept its supported bit depth for this colour mode."];
    }

    ICScannerMeasurementUnit unit = functionalUnit.measurementUnit;
    if ([functionalUnit.supportedMeasurementUnits
            containsIndex:ICScannerMeasurementUnitCentimeters]) {
      unit = ICScannerMeasurementUnitCentimeters;
      functionalUnit.measurementUnit = unit;
    } else if ([functionalUnit.supportedMeasurementUnits
                   containsIndex:ICScannerMeasurementUnitInches]) {
      unit = ICScannerMeasurementUnitInches;
      functionalUnit.measurementUnit = unit;
    }
    CGFloat width = [self measurementValueForMillimetres:[self.request[@"paperWidthMm"] doubleValue]
                                                    unit:unit
                                                     dpi:selectedDpi];
    CGFloat height = [self measurementValueForMillimetres:[self.request[@"paperHeightMm"] doubleValue]
                                                     unit:unit
                                                      dpi:selectedDpi];
    NSSize physicalSize = functionalUnit.physicalSize;
    BOOL constrained = NO;
    if (physicalSize.width > 0 && width > physicalSize.width) {
      width = physicalSize.width;
      constrained = YES;
    }
    if (physicalSize.height > 0 && height > physicalSize.height) {
      height = physicalSize.height;
      constrained = YES;
    }
    functionalUnit.scanArea = NSMakeRect(0, 0, MAX(width, 1), MAX(height, 1));
    if (constrained) {
      [self.warnings addObject:@"The requested paper size was constrained to the scanner bed."];
    }

    if ([self.request[@"source"] isEqualToString:@"feeder"]) {
      if (![functionalUnit
              isKindOfClass:[ICScannerFunctionalUnitDocumentFeeder class]]) {
        *errorMessage = @"Image Capture did not select a document feeder.";
        return NO;
      }
      ICScannerFunctionalUnitDocumentFeeder *feeder =
          (ICScannerFunctionalUnitDocumentFeeder *)functionalUnit;
      BOOL duplex = [self.request[@"duplex"] boolValue];
      if (duplex && !feeder.supportsDuplexScanning) {
        *errorMessage = @"The selected feeder does not support duplex scanning.";
        return NO;
      }
      feeder.duplexScanningEnabled = duplex;
      if (!feeder.documentLoaded) {
        *errorMessage = @"Load at least one page into the scanner document feeder.";
        return NO;
      }
    }
  } @catch (NSException *exception) {
    *errorMessage = exception.reason.length > 0
                        ? [NSString stringWithFormat:@"The scanner rejected its settings: %@",
                                                     exception.reason]
                        : @"The scanner rejected the requested settings.";
    return NO;
  }
  return YES;
}

- (void)beginScanWithFunctionalUnit:(ICScannerFunctionalUnit *)functionalUnit {
  NSString *errorMessage = nil;
  if (![self configureFunctionalUnit:functionalUnit error:&errorMessage]) {
    [self fail:errorMessage ?: @"The scanner settings could not be applied."];
    return;
  }
  NSString *outputPath = self.request[@"outputDirectory"];
  self.activeScanner.transferMode = ICScannerTransferModeFileBased;
  self.activeScanner.downloadsDirectory = [NSURL fileURLWithPath:outputPath isDirectory:YES];
  self.activeScanner.documentName = @"scan";
  self.activeScanner.documentUTI = @"public.tiff";
  self.phase = TPPhaseCaptureScanning;
  [self.activeScanner requestScan];
}

- (BOOL)isURLInsideOutputDirectory:(NSURL *)url {
  NSString *output = [self.request[@"outputDirectory"] stringByStandardizingPath];
  NSString *parent = [url.URLByDeletingLastPathComponent.path stringByStandardizingPath];
  return [output isEqualToString:parent];
}

- (void)discardExtraOutputURL:(NSURL *)url {
  if (url.isFileURL && [self isURLInsideOutputDirectory:url]) {
    [[NSFileManager defaultManager] removeItemAtURL:url error:nil];
  }
}

- (void)storeScannedURL:(NSURL *)sourceURL {
  NSUInteger pageLimit = [self.request[@"pageLimit"] unsignedIntegerValue];
  BOOL feederCapture = [self.request[@"source"] isEqualToString:@"feeder"];
  if (self.capturePaths.count >= pageLimit) {
    self.cancelledAtPageLimit = feederCapture;
    [self discardExtraOutputURL:sourceURL];
    if (feederCapture) {
      [self.activeScanner cancelScan];
    }
    return;
  }
  if (!sourceURL.isFileURL) {
    [self.activeScanner cancelScan];
    [self fail:@"Image Capture returned a non-file scan result."];
    return;
  }

  NSString *extension = sourceURL.pathExtension.lowercaseString;
  NSSet<NSString *> *supportedExtensions =
      [NSSet setWithArray:@[ @"bmp", @"jpeg", @"jpg", @"png", @"tif", @"tiff" ]];
  if (![supportedExtensions containsObject:extension]) {
    extension = @"tiff";
  }
  NSString *filename = [NSString stringWithFormat:@"scan-%04lu.%@",
                                                   (unsigned long)(self.capturePaths.count + 1),
                                                   extension];
  NSURL *outputDirectory =
      [NSURL fileURLWithPath:self.request[@"outputDirectory"] isDirectory:YES];
  NSURL *destinationURL = [outputDirectory URLByAppendingPathComponent:filename];
  NSError *fileError = nil;
  if (![sourceURL.path isEqualToString:destinationURL.path]) {
    if (![[NSFileManager defaultManager] copyItemAtURL:sourceURL
                                                 toURL:destinationURL
                                                 error:&fileError]) {
      [self.activeScanner cancelScan];
      [self fail:[NSString stringWithFormat:@"A scanned page could not be stored: %@",
                                                TPErrorDescription(fileError, @"unknown file error")]];
      return;
    }
    if ([self isURLInsideOutputDirectory:sourceURL]) {
      [[NSFileManager defaultManager] removeItemAtURL:sourceURL error:nil];
    }
  }
  [self.capturePaths addObject:destinationURL.path];
  if (feederCapture && self.capturePaths.count >= pageLimit) {
    self.cancelledAtPageLimit = YES;
    [self.activeScanner cancelScan];
  }
}

- (void)completeCaptureWithError:(NSError *)error {
  if (self.capturePaths.count == 0) {
    [self fail:error != nil ? TPErrorDescription(error, @"The scanner returned no pages.")
                            : @"The scanner returned no pages."];
    return;
  }
  if (error != nil && !self.cancelledAtPageLimit) {
    [self fail:TPErrorDescription(error, @"Image Capture could not complete the scan.")];
    return;
  }
  if (self.cancelledAtPageLimit) {
    [self.warnings addObject:@"Scanning stopped at the requested page limit."];
  }
  if (self.activeScanner.hasOpenSession) {
    self.phase = TPPhaseCaptureClosing;
    [self.activeScanner requestCloseSession];
  } else {
    [self emitCapture];
  }
}

- (void)emitDiscovery {
  [self emitObject:@{
    @"protocolVersion" : @(TPProtocolVersion),
    @"devices" : self.deviceRecords
  }];
}

- (void)emitCapture {
  [self emitObject:@{
    @"protocolVersion" : @(TPProtocolVersion),
    @"paths" : self.capturePaths,
    @"warnings" : self.warnings
  }];
}

- (void)emitObject:(NSDictionary *)object {
  if (self.finished) {
    return;
  }
  NSError *error = nil;
  NSData *data = [NSJSONSerialization dataWithJSONObject:object
                                                 options:NSJSONWritingSortedKeys
                                                   error:&error];
  if (data == nil) {
    [self fail:[NSString stringWithFormat:@"The scanner result could not be encoded: %@",
                                              TPErrorDescription(error, @"unknown JSON error")]];
    return;
  }
  [self.deadlineTimer invalidate];
  [self.settleTimer invalidate];
  fwrite(data.bytes, 1, data.length, stdout);
  fputc('\n', stdout);
  fflush(stdout);
  self.phase = TPPhaseFinished;
  self.exitCode = 0;
  self.finished = YES;
}

- (void)fail:(NSString *)message {
  if (self.finished) {
    return;
  }
  [self.deadlineTimer invalidate];
  [self.settleTimer invalidate];
  [self.browser stop];
  if (self.phase == TPPhaseCaptureScanning) {
    [self.activeScanner cancelScan];
  }
  const char *diagnostic = message.length > 0 ? message.UTF8String : "Image Capture failed.";
  fprintf(stderr, "%s\n", diagnostic);
  fflush(stderr);
  self.phase = TPPhaseFinished;
  self.exitCode = 2;
  self.finished = YES;
}

#pragma mark - ICDeviceBrowserDelegate

- (void)deviceBrowser:(ICDeviceBrowser *)browser
          didAddDevice:(ICDevice *)device
            moreComing:(BOOL)moreComing {
  (void)browser;
  if (self.phase != TPPhaseBrowsing ||
      ![device isKindOfClass:[ICScannerDevice class]]) {
    return;
  }
  ICScannerDevice *scanner = (ICScannerDevice *)device;
  NSString *identifier = TPDeviceIdentifier(scanner);
  if (identifier.length == 0) {
    return;
  }
  if (self.operation == TPOperationCapture) {
    if ([identifier isEqualToString:self.request[@"deviceId"]]) {
      [self openCaptureScanner:scanner];
    }
    return;
  }
  BOOL alreadyPresent = NO;
  for (ICScannerDevice *candidate in self.foundScanners) {
    if ([TPDeviceIdentifier(candidate) isEqualToString:identifier]) {
      alreadyPresent = YES;
      break;
    }
  }
  if (!alreadyPresent) {
    [self.foundScanners addObject:scanner];
  }
  if (!moreComing) {
    [self restartDiscoverySettleTimer];
  }
}

- (void)deviceBrowser:(ICDeviceBrowser *)browser
       didRemoveDevice:(ICDevice *)device
             moreGoing:(BOOL)moreGoing {
  (void)browser;
  (void)moreGoing;
  if (device == self.activeScanner && self.operation == TPOperationCapture) {
    [self fail:@"The scanner was disconnected during capture."];
  }
}

- (void)deviceBrowserDidEnumerateLocalDevices:(ICDeviceBrowser *)browser {
  (void)browser;
  if (self.operation == TPOperationDiscover && self.phase == TPPhaseBrowsing) {
    [self restartDiscoverySettleTimer];
  }
}

#pragma mark - ICDeviceDelegate

- (void)device:(ICDevice *)device didOpenSessionWithError:(NSError *)error {
  if (device != self.activeScanner) {
    return;
  }
  if (self.phase == TPPhaseDiscoveryOpening) {
    if (error != nil) {
      [self.currentDetailParts
          addObject:@"Capabilities could not be inspected because the scanner session did not open."];
      [self finishCurrentDiscoveryRecord];
      return;
    }
    [self prepareDiscoveryFunctionalUnits];
    [self selectNextDiscoveryFunctionalUnit];
    return;
  }
  if (self.phase == TPPhaseCaptureOpening) {
    if (error != nil) {
      [self fail:[NSString stringWithFormat:@"The scanner session could not be opened: %@",
                                                TPErrorDescription(error, @"unknown session error")]];
      return;
    }
    [self selectCaptureFunctionalUnit];
  }
}

- (void)device:(ICDevice *)device didCloseSessionWithError:(NSError *)error {
  if (device != self.activeScanner) {
    return;
  }
  if (self.phase == TPPhaseDiscoveryClosing) {
    if (error != nil) {
      self.currentRecord[@"detail"] = @"Image Capture reported an error while closing the scanner session.";
    }
    [self moveToNextDiscoveryScanner];
  } else if (self.phase == TPPhaseCaptureClosing) {
    if (error != nil) {
      [self.warnings addObject:@"Image Capture reported an error while closing the scanner session."];
    }
    [self emitCapture];
  }
}

- (void)didRemoveDevice:(ICDevice *)device {
  if (device == self.activeScanner && self.operation == TPOperationCapture) {
    [self fail:@"The scanner was disconnected during capture."];
  }
}

- (void)device:(ICDevice *)device didEncounterError:(NSError *)error {
  if (device == self.activeScanner && self.operation == TPOperationCapture &&
      !self.cancelledAtPageLimit) {
    [self fail:TPErrorDescription(error, @"Image Capture reported a scanner error.")];
  }
}

#pragma mark - ICScannerDeviceDelegate

- (void)scannerDevice:(ICScannerDevice *)scanner
    didSelectFunctionalUnit:(ICScannerFunctionalUnit *)functionalUnit
                     error:(NSError *)error {
  if (scanner != self.activeScanner) {
    return;
  }
  if (self.phase == TPPhaseDiscoverySelecting) {
    if (error != nil || functionalUnit == nil) {
      [self.currentDetailParts addObject:@"One scanner source could not be inspected."];
    } else {
      [self inspectFunctionalUnit:functionalUnit];
    }
    [self selectNextDiscoveryFunctionalUnit];
  } else if (self.phase == TPPhaseCaptureSelecting) {
    if (error != nil || functionalUnit == nil) {
      [self fail:error != nil
                     ? [NSString stringWithFormat:@"The scanner source could not be selected: %@",
                                                        TPErrorDescription(error, @"unknown source error")]
                     : @"The scanner source could not be selected."];
      return;
    }
    [self beginScanWithFunctionalUnit:functionalUnit];
  }
}

- (void)scannerDevice:(ICScannerDevice *)scanner didScanToURL:(NSURL *)url {
  if (scanner == self.activeScanner && self.phase == TPPhaseCaptureScanning) {
    [self storeScannedURL:url];
  }
}

- (void)scannerDevice:(ICScannerDevice *)scanner
    didCompleteScanWithError:(NSError *)error {
  if (scanner == self.activeScanner && self.phase == TPPhaseCaptureScanning) {
    [self completeCaptureWithError:error];
  }
}

@end

static NSDictionary *TPReadCaptureRequest(void) {
  NSData *data = [[NSFileHandle fileHandleWithStandardInput] readDataToEndOfFile];
  if (data.length == 0 || data.length > 64 * 1024) {
    return nil;
  }
  NSError *error = nil;
  id value = [NSJSONSerialization JSONObjectWithData:data options:0 error:&error];
  if (error != nil || ![value isKindOfClass:[NSDictionary class]]) {
    return nil;
  }
  NSDictionary *request = (NSDictionary *)value;
  NSNumber *protocolVersion = request[@"protocolVersion"];
  NSString *deviceId = request[@"deviceId"];
  NSString *source = request[@"source"];
  NSString *colourMode = request[@"colourMode"];
  NSString *outputDirectory = request[@"outputDirectory"];
  NSNumber *dpi = request[@"dpi"];
  NSNumber *duplex = request[@"duplex"];
  NSNumber *width = request[@"paperWidthMm"];
  NSNumber *height = request[@"paperHeightMm"];
  NSNumber *pageLimit = request[@"pageLimit"];
  NSSet *sources = [NSSet setWithArray:@[ @"flatbed", @"feeder" ]];
  NSSet *colourModes =
      [NSSet setWithArray:@[ @"colour", @"greyscale", @"monochrome" ]];
  NSSet *expectedKeys = [NSSet setWithArray:@[
    @"protocolVersion", @"deviceId", @"source", @"duplex", @"dpi", @"colourMode",
    @"paperWidthMm", @"paperHeightMm", @"pageLimit", @"outputDirectory"
  ]];
  NSSet *actualKeys = [NSSet setWithArray:request.allKeys];
  BOOL validTypes = [protocolVersion isKindOfClass:[NSNumber class]] &&
                    [deviceId isKindOfClass:[NSString class]] &&
                    [source isKindOfClass:[NSString class]] &&
                    [colourMode isKindOfClass:[NSString class]] &&
                    [outputDirectory isKindOfClass:[NSString class]] &&
                    [dpi isKindOfClass:[NSNumber class]] &&
                    [duplex isKindOfClass:[NSNumber class]] &&
                    [width isKindOfClass:[NSNumber class]] &&
                    [height isKindOfClass:[NSNumber class]] &&
                    [pageLimit isKindOfClass:[NSNumber class]];
  BOOL isDirectory = NO;
  BOOL validDirectory = validTypes && outputDirectory.length > 0 &&
                        [[NSFileManager defaultManager] fileExistsAtPath:outputDirectory
                                                            isDirectory:&isDirectory] &&
                        isDirectory;
  BOOL valid = [actualKeys isEqualToSet:expectedKeys] &&
               validTypes &&
               protocolVersion.unsignedIntegerValue == TPProtocolVersion &&
               deviceId.length > 0 && deviceId.length <= 4096 &&
               [sources containsObject:source] && [colourModes containsObject:colourMode] &&
               validDirectory && dpi.unsignedIntegerValue >= 75 &&
               dpi.unsignedIntegerValue <= 1200 && width.doubleValue >= 10.0 &&
               width.doubleValue <= 500.0 && height.doubleValue >= 10.0 &&
               height.doubleValue <= 500.0 && pageLimit.unsignedIntegerValue >= 1 &&
               pageLimit.unsignedIntegerValue <= 200 &&
               !([source isEqualToString:@"flatbed"] && pageLimit.unsignedIntegerValue != 1) &&
               !([duplex boolValue] && ![source isEqualToString:@"feeder"]);
  return valid ? request : nil;
}

int main(int argc, const char *argv[]) {
  @autoreleasepool {
    if (argc != 2) {
      fprintf(stderr, "Use either the discover or capture operation.\n");
      return 2;
    }
    NSString *operationName = [NSString stringWithUTF8String:argv[1]];
    TPOperation operation;
    NSDictionary *request = @{};
    if ([operationName isEqualToString:@"discover"]) {
      operation = TPOperationDiscover;
    } else if ([operationName isEqualToString:@"capture"]) {
      operation = TPOperationCapture;
      request = TPReadCaptureRequest();
      if (request == nil) {
        fprintf(stderr, "The Image Capture request was invalid.\n");
        return 2;
      }
    } else {
      fprintf(stderr, "The Image Capture operation was not recognised.\n");
      return 2;
    }

    TPScannerBridge *bridge = [[TPScannerBridge alloc] initWithOperation:operation
                                                                 request:request];
    [bridge start];
    while (!bridge.finished) {
      @autoreleasepool {
        (void)[[NSRunLoop currentRunLoop]
            runMode:NSDefaultRunLoopMode
         beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.1]];
      }
    }
    return bridge.exitCode;
  }
}

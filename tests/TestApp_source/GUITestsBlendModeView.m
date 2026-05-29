/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

#include "system_headers.h"

#include "GUITestsAppDelegate.h"
#include "GUITestsBlendModeView.h"

#define NUM_TESTS 2

// Filled with a yellow background and a cyan overlay rectangle. The blend mode
// chosen for the overlay determines the visible color where the two rectangles
// intersect:
//   - kCGBlendModeNormal:   the overlap shows the overlay color (cyan).
//   - kCGBlendModeMultiply: the overlap shows yellow * cyan = green.
@interface GUITestsBlendModeTestArea : UIView {
@public
  CGBlendMode blendMode;
}
@end

@implementation GUITestsBlendModeTestArea : UIView
- (void)drawRect:(CGRect)rect {
  CGContextRef context = UIGraphicsGetCurrentContext();
  CGRect bounds = [self bounds];

  // Background fill: gray, so the surrounding test area color is visible too.
  CGContextSetRGBFillColor(context, 0.5, 0.5, 0.5, 1.0);
  CGContextFillRect(context, bounds);

  // First (base) rectangle: opaque yellow in the upper-left.
  CGContextSetBlendMode(context, kCGBlendModeNormal);
  CGContextSetRGBFillColor(context, 1.0, 1.0, 0.0, 1.0);
  CGContextFillRect(context, CGRectMake(40, 40, 180, 180));

  // Second (overlay) rectangle: opaque cyan, shifted so it overlaps the
  // yellow rectangle. The blend mode under test controls how the overlap
  // composites.
  CGContextSetBlendMode(context, blendMode);
  CGContextSetRGBFillColor(context, 0.0, 1.0, 1.0, 1.0);
  CGContextFillRect(context, CGRectMake(120, 120, 180, 180));

  // Restore the default blend mode so later drawing isn't affected.
  CGContextSetBlendMode(context, kCGBlendModeNormal);
}
@end

@implementation GUITestsBlendModeView : UIView

UILabel *blendTitle;
UILabel *expectationLabel;
GUITestsBlendModeTestArea *blendTestArea;
NSUInteger blendTestNum;

- (instancetype)initWithFrame:(CGRect)frame {
  [super initWithFrame:frame];

  blendTitle = [[UILabel alloc] initWithFrame:CGRectMake(0, 0, 320, 20)];
  blendTitle.text =
      [NSString stringWithUTF8String:"CGContextSetBlendMode (press →)"];
  blendTitle.textAlignment = UITextAlignmentCenter;
  [self addSubview:blendTitle];

  expectationLabel =
      [[UILabel alloc] initWithFrame:CGRectMake(0, 380, 320, 20)];
  expectationLabel.textAlignment = UITextAlignmentCenter;
  expectationLabel.textColor = [UIColor whiteColor];
  expectationLabel.backgroundColor = [UIColor clearColor];
  [self addSubview:expectationLabel];

  UIButton *button1 = [UIButton buttonWithType:UIButtonTypeRoundedRect];
  [button1 setTitle:[NSString stringWithUTF8String:"←"]
           forState:UIControlStateNormal];
  [button1 setFrame:CGRectMake(0, 420, 40, 40)];
  [button1 addTarget:self
                action:@selector(prevTest)
      forControlEvents:UIControlEventTouchUpInside];
  [self addSubview:button1];
  [button1 layoutSubviews]; // FIXME: workaround for touchHLE not calling this

  UIButton *button2 = [UIButton buttonWithType:UIButtonTypeRoundedRect];
  [button2 setTitle:[NSString stringWithUTF8String:"→"]
           forState:UIControlStateNormal];
  [button2 setFrame:CGRectMake(280, 420, 40, 40)];
  [button2 addTarget:self
                action:@selector(nextTest)
      forControlEvents:UIControlEventTouchUpInside];
  [self addSubview:button2];
  [button2 layoutSubviews]; // FIXME: workaround for touchHLE not calling this

  blendTestNum = 0;

  return self;
}

- (void)dealloc {
  [blendTitle release];
  [expectationLabel release];
  [blendTestArea release];
  [super dealloc];
}

- (void)prevTest {
  if (blendTestNum > 1)
    blendTestNum--;
  [self displayTest];
}

- (void)nextTest {
  if (blendTestNum < NUM_TESTS)
    blendTestNum++;
  [self displayTest];
}

- (void)displayTest {
  blendTitle.text = [NSString
      stringWithFormat:[NSString stringWithUTF8String:"BlendMode test %u/%u"],
                       blendTestNum, NUM_TESTS];
  [blendTestArea removeFromSuperview];
  [blendTestArea release];
  blendTestArea = [[GUITestsBlendModeTestArea alloc]
      initWithFrame:CGRectMake(10, 30, 300, 340)];
  blendTestArea.backgroundColor = [UIColor blackColor];
  [self addSubview:blendTestArea];
  [blendTestArea setNeedsDisplay]; // FIXME: normally we should not need that...

  [self performSelector:NSSelectorFromString([NSString
                            stringWithFormat:[NSString
                                                 stringWithUTF8String:"test%u"],
                                             blendTestNum])];
}

// Test 1: kCGBlendModeNormal. The cyan overlay should fully replace the yellow
// rectangle where the two rectangles intersect.
- (void)test1 {
  blendTestArea->blendMode = kCGBlendModeNormal;
  expectationLabel.text =
      [NSString stringWithUTF8String:"Normal: overlap is cyan"];
}

// Test 2: kCGBlendModeMultiply. Where the rectangles overlap, the colors
// multiply: (1, 1, 0) * (0, 1, 1) = (0, 1, 0), i.e. green.
- (void)test2 {
  blendTestArea->blendMode = kCGBlendModeMultiply;
  expectationLabel.text =
      [NSString stringWithUTF8String:"Multiply: overlap is green"];
}

@end

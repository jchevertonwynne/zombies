//! Zones: which one the Player is in, and which are Active.

use bevy::prelude::*;

use crate::debug::DebugView;
use crate::maps::door::Door;
use crate::maps::map::{MapId, STOREY};
use crate::players::player::{LocalPlayer, Player};
use crate::players::respawn::AwaitingRespawn;
use crate::state::GameState;
use crate::tuning::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZoneId {
    StartRoom,
    WestHall,
    Balcony,
    EastHall,
    DiningRoom,
    PowerRoom,
    Yard,
    /// The corridor south from the Dining Room to the Workshop.
    Passage,
    Workshop,
    /// Between the Workshop and the Generator Room, at the middle of the south wing.
    Storeroom,
    GeneratorRoom,
    /// The corridor south from the Power Room to the Generator Room.
    Tunnel,
    /// The Quarry's start: a brick pump house at the foot of the pit.
    PumpHouse,
    /// The open pit floor, with the sump pond and the Risers.
    QuarryFloor,
    /// The first terrace's open sidings, up the ramp from the pit floor.
    RailYard,
    /// The shed on the first terrace, round the stone crusher.
    CrusherShed,
    /// The engine house on the second terrace, round its winding drum.
    WindingHouse,
    /// The mine offices on the second terrace.
    Offices,
    /// The pithead tower: a yard at the second terrace's level and the deck above it.
    Headframe,
    /// The rock cutting up the pit's west side, climbing a landing at each terrace.
    Incline,
    /// The goods shed off the pit's east side, with its loading platform.
    LoadingDock,
    /// The Village's start: the inn in the middle of the village, between the Market and the
    /// Back Lane.
    Inn,
    /// The open market place in the middle of the High Street, round its market cross.
    Market,
    /// The High Street west of the Market, out to the Mill.
    WestEnd,
    /// The High Street east of the Market, out to the Church.
    EastEnd,
    /// The lane behind the Inn, gated off from the lanes either side until the Power comes on.
    BackLane,
    /// The lane behind the west end of the village, out to the Orchard.
    MillLane,
    /// The lane behind the east end of the village, out to the Graveyard.
    ChurchLane,
    /// The watermill at the west end, with the Power Switch.
    Mill,
    /// The smithy, between the West End and Mill Lane.
    Smithy,
    /// The bakery, between the Market and the Back Lane.
    Bakery,
    /// The schoolhouse, between the East End and Church Lane.
    Schoolhouse,
    /// The church at the east end, with Pack-a-Punch on its chancel.
    Church,
    /// The orchard off Mill Lane.
    Orchard,
    /// The churchyard off Church Lane, round the mausoleum.
    Graveyard,
    /// The Dam's start: the road along its crest, the only Zone out of doors.
    CrestRoad,
    /// The top of the intake tower, at the west end of the crest.
    TowerHead,
    /// The stairwell down the intake tower, from the Tower Head to the Intake Gallery.
    GateShaft,
    /// The gallery inside the dam from the foot of the Gate Shaft, along to the Control Room and
    /// down the dam's west side to the Drainage Gallery.
    IntakeGallery,
    /// The gatehouse over the spillway, at the east end of the crest.
    SpillwayGatehouse,
    /// The gallery inside the dam under the Spillway Gatehouse, along to the Control Room and down
    /// the dam's east side to the Spillway Stair.
    SpillwayGallery,
    /// The stairs down from the Spillway Gallery to the Drainage Gallery.
    SpillwayStair,
    /// The control room in the middle of the dam, with Pack-a-Punch on its dais.
    ControlRoom,
    /// The tunnel and stairs down beside the penstock, from the Control Room to the Turbine Hall.
    PenstockTunnel,
    /// The powerhouse at the dam's foot, round its turbine deck.
    TurbineHall,
    /// The room of breaker cabinets off the Turbine Hall.
    SwitchgearRoom,
    /// The gallery along the dam's toe, from under the Intake Gallery to the foot of the Spillway
    /// Stair.
    DrainageGallery,
    /// The Cove's Pack-a-Punch: a rock cave at the west end of the shore, behind the Power Door.
    SeaCave,
    /// The pebble beach west of the Strand, under the Fishermen's Steps.
    Shingle,
    /// The Cove's start: the sand along the foot of the town, with Quick Revive in its sea wall.
    Strand,
    /// The roofed boathouse at the east end of the shore.
    Boathouse,
    /// The roofed washhouse at the west end of the Lower Town, under the Chapel Stair.
    Washhouse,
    /// The lane along the Lower Town's contour, up the Fishermen's Steps from the Shingle.
    LowerLane,
    /// The open-sided loggia of the Lower Town, up the Sea Wall Steps from the Strand.
    FishMarket,
    /// The roofed taverna with its pergola deck, up the Boathouse Stair.
    Taverna,
    /// The chapel of the Upper Town, up the Chapel Stair, with its bell tower.
    Chapel,
    /// The square of the Upper Town round its fountain, up the Long Stair.
    Piazza,
    /// The lane along the Upper Town's contour.
    UpperLane,
    /// The guesthouse at the east end of the Upper Town, with a roof garden.
    Pensione,
    /// The lighthouse on the Headland, with the Power Switch at its foot and its lantern gallery above.
    Lighthouse,
    /// The yard at the foot of the lighthouse, up the Pilgrims' Stair.
    LighthouseYard,
    /// The lighthouse keeper's cottage.
    KeepersCottage,
    /// The old gun battery at the east end of the Headland, up the Cliff Stair.
    Battery,
    /// The Polder's start: the square on the south bank of the Town Canal, round its pump.
    TownSquare,
    /// The roofed weigh house west of the square, with Ford F1 in front of it.
    WeighHouse,
    /// The brewhouse and its yard at the west end of the south bank, at the Humpback Bridge.
    Brewery,
    /// The cheese market east of the square, with Ford F2.
    CheeseMarket,
    /// The court of almshouses behind the square.
    AlmshouseCourt,
    /// The mill yard at the west end of the Harbour, with a stage at the dyke's height.
    Windmill,
    /// The roofed warehouse between the two Canals, with a water door onto each.
    Warehouse,
    /// The yard along the lock's west chamber, with the Inland Gate.
    LockYard,
    /// The island between the lock chambers, with Pack-a-Punch.
    SluiceHouse,
    /// The quay along the lock's east chamber, with the Sea Gate.
    TollQuay,
    /// The shipyard east of the lock, with Ford F4's far bank.
    Shipyard,
    /// The coal yard at the Harbour's east end, under the Coal Stair.
    CoalYard,
    /// The herring quay on the Harbour's north bank, with Ford F3.
    HerringQuay,
    /// The customs house on the Harbour's north bank, with Ford F4.
    CustomsHouse,
    /// The road along the sea dyke's crest.
    DykeRoad,
    /// The roofed engine house astride the dyke, with the Power Switch.
    PumpingStation,
}

impl ZoneId {
    pub const ALL: [ZoneId; 79] = [
        ZoneId::StartRoom,
        ZoneId::WestHall,
        ZoneId::Balcony,
        ZoneId::EastHall,
        ZoneId::DiningRoom,
        ZoneId::PowerRoom,
        ZoneId::Yard,
        ZoneId::Passage,
        ZoneId::Workshop,
        ZoneId::Storeroom,
        ZoneId::GeneratorRoom,
        ZoneId::Tunnel,
        ZoneId::PumpHouse,
        ZoneId::QuarryFloor,
        ZoneId::RailYard,
        ZoneId::CrusherShed,
        ZoneId::WindingHouse,
        ZoneId::Offices,
        ZoneId::Headframe,
        ZoneId::Incline,
        ZoneId::LoadingDock,
        ZoneId::Inn,
        ZoneId::Market,
        ZoneId::WestEnd,
        ZoneId::EastEnd,
        ZoneId::BackLane,
        ZoneId::MillLane,
        ZoneId::ChurchLane,
        ZoneId::Mill,
        ZoneId::Smithy,
        ZoneId::Bakery,
        ZoneId::Schoolhouse,
        ZoneId::Church,
        ZoneId::Orchard,
        ZoneId::Graveyard,
        ZoneId::CrestRoad,
        ZoneId::TowerHead,
        ZoneId::GateShaft,
        ZoneId::IntakeGallery,
        ZoneId::SpillwayGatehouse,
        ZoneId::SpillwayGallery,
        ZoneId::SpillwayStair,
        ZoneId::ControlRoom,
        ZoneId::PenstockTunnel,
        ZoneId::TurbineHall,
        ZoneId::SwitchgearRoom,
        ZoneId::DrainageGallery,
        ZoneId::SeaCave,
        ZoneId::Shingle,
        ZoneId::Strand,
        ZoneId::Boathouse,
        ZoneId::Washhouse,
        ZoneId::LowerLane,
        ZoneId::FishMarket,
        ZoneId::Taverna,
        ZoneId::Chapel,
        ZoneId::Piazza,
        ZoneId::UpperLane,
        ZoneId::Pensione,
        ZoneId::Lighthouse,
        ZoneId::LighthouseYard,
        ZoneId::KeepersCottage,
        ZoneId::Battery,
        ZoneId::TownSquare,
        ZoneId::WeighHouse,
        ZoneId::Brewery,
        ZoneId::CheeseMarket,
        ZoneId::AlmshouseCourt,
        ZoneId::Windmill,
        ZoneId::Warehouse,
        ZoneId::LockYard,
        ZoneId::SluiceHouse,
        ZoneId::TollQuay,
        ZoneId::Shipyard,
        ZoneId::CoalYard,
        ZoneId::HerringQuay,
        ZoneId::CustomsHouse,
        ZoneId::DykeRoad,
        ZoneId::PumpingStation,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ZoneId::StartRoom => "Start Room",
            ZoneId::WestHall => "West Hall",
            ZoneId::Balcony => "Balcony",
            ZoneId::EastHall => "East Hall",
            ZoneId::DiningRoom => "Dining Room",
            ZoneId::PowerRoom => "Power Room",
            ZoneId::Yard => "Yard",
            ZoneId::Passage => "Passage",
            ZoneId::Workshop => "Workshop",
            ZoneId::Storeroom => "Storeroom",
            ZoneId::GeneratorRoom => "Generator Room",
            ZoneId::Tunnel => "Tunnel",
            ZoneId::PumpHouse => "Pump House",
            ZoneId::QuarryFloor => "Quarry Floor",
            ZoneId::RailYard => "Rail Yard",
            ZoneId::CrusherShed => "Crusher Shed",
            ZoneId::WindingHouse => "Winding House",
            ZoneId::Offices => "Offices",
            ZoneId::Headframe => "Headframe",
            ZoneId::Incline => "Incline",
            ZoneId::LoadingDock => "Loading Dock",
            ZoneId::Inn => "Inn",
            ZoneId::Market => "Market",
            ZoneId::WestEnd => "West End",
            ZoneId::EastEnd => "East End",
            ZoneId::BackLane => "Back Lane",
            ZoneId::MillLane => "Mill Lane",
            ZoneId::ChurchLane => "Church Lane",
            ZoneId::Mill => "Mill",
            ZoneId::Smithy => "Smithy",
            ZoneId::Bakery => "Bakery",
            ZoneId::Schoolhouse => "Schoolhouse",
            ZoneId::Church => "Church",
            ZoneId::Orchard => "Orchard",
            ZoneId::Graveyard => "Graveyard",
            ZoneId::CrestRoad => "Crest Road",
            ZoneId::TowerHead => "Tower Head",
            ZoneId::GateShaft => "Gate Shaft",
            ZoneId::IntakeGallery => "Intake Gallery",
            ZoneId::SpillwayGatehouse => "Spillway Gatehouse",
            ZoneId::SpillwayGallery => "Spillway Gallery",
            ZoneId::SpillwayStair => "Spillway Stair",
            ZoneId::ControlRoom => "Control Room",
            ZoneId::PenstockTunnel => "Penstock Tunnel",
            ZoneId::TurbineHall => "Turbine Hall",
            ZoneId::SwitchgearRoom => "Switchgear Room",
            ZoneId::DrainageGallery => "Drainage Gallery",
            ZoneId::SeaCave => "Sea Cave",
            ZoneId::Shingle => "Shingle",
            ZoneId::Strand => "Strand",
            ZoneId::Boathouse => "Boathouse",
            ZoneId::Washhouse => "Washhouse",
            ZoneId::LowerLane => "Lower Lane",
            ZoneId::FishMarket => "Fish Market",
            ZoneId::Taverna => "Taverna",
            ZoneId::Chapel => "Chapel",
            ZoneId::Piazza => "Piazza",
            ZoneId::UpperLane => "Upper Lane",
            ZoneId::Pensione => "Pensione",
            ZoneId::Lighthouse => "Lighthouse",
            ZoneId::LighthouseYard => "Lighthouse Yard",
            ZoneId::KeepersCottage => "Keeper's Cottage",
            ZoneId::Battery => "Battery",
            ZoneId::TownSquare => "Town Square",
            ZoneId::WeighHouse => "Weigh House",
            ZoneId::Brewery => "Brewery",
            ZoneId::CheeseMarket => "Cheese Market",
            ZoneId::AlmshouseCourt => "Almshouse Court",
            ZoneId::Windmill => "Windmill",
            ZoneId::Warehouse => "Warehouse",
            ZoneId::LockYard => "Lock Yard",
            ZoneId::SluiceHouse => "Sluice House",
            ZoneId::TollQuay => "Toll Quay",
            ZoneId::Shipyard => "Shipyard",
            ZoneId::CoalYard => "Coal Yard",
            ZoneId::HerringQuay => "Herring Quay",
            ZoneId::CustomsHouse => "Customs House",
            ZoneId::DykeRoad => "Dyke Road",
            ZoneId::PumpingStation => "Pumping Station",
        }
    }

    /// The Map the Zone is part of.
    pub fn map(self) -> MapId {
        use ZoneId::*;
        match self {
            StartRoom | WestHall | Balcony | EastHall | DiningRoom | PowerRoom | Yard | Passage | Workshop | Storeroom | GeneratorRoom | Tunnel => MapId::Greybox,
            PumpHouse | QuarryFloor | RailYard | CrusherShed | WindingHouse | Offices | Headframe | Incline | LoadingDock => MapId::Quarry,
            Inn | Market | WestEnd | EastEnd | BackLane | MillLane | ChurchLane | Mill | Smithy | Bakery | Schoolhouse | Church | Orchard | Graveyard => MapId::Village,
            CrestRoad | TowerHead | GateShaft | IntakeGallery | SpillwayGatehouse | SpillwayGallery | SpillwayStair | ControlRoom | PenstockTunnel | TurbineHall
            | SwitchgearRoom | DrainageGallery => MapId::Dam,
            SeaCave | Shingle | Strand | Boathouse | Washhouse | LowerLane | FishMarket | Taverna | Chapel | Piazza | UpperLane | Pensione | Lighthouse | LighthouseYard
            | KeepersCottage | Battery => MapId::Cove,
            TownSquare | WeighHouse | Brewery | CheeseMarket | AlmshouseCourt | Windmill | Warehouse | LockYard | SluiceHouse | TollQuay | Shipyard | CoalYard | HerringQuay
            | CustomsHouse | DykeRoad | PumpingStation => MapId::Polder,
        }
    }

    /// The height of the Zone's floor: the ground, a storey up for the Balcony, and a terrace's
    /// height in the Quarry. The Village is all on the ground. The Dam has three levels: the foot,
    /// the gallery a storey up and the crest two up. The Cove has four: the Shore, the Lower Town,
    /// the Upper Town and the Headland, a storey apart.
    pub fn floor(self) -> f32 {
        use ZoneId::*;
        match self {
            StartRoom | WestHall | EastHall | DiningRoom | PowerRoom | Yard | Passage | Workshop | Storeroom | GeneratorRoom | Tunnel => 0.0,
            PumpHouse | QuarryFloor | Incline | LoadingDock => 0.0,
            Inn | Market | WestEnd | EastEnd | BackLane | MillLane | ChurchLane | Mill | Smithy | Bakery | Schoolhouse | Church | Orchard | Graveyard => 0.0,
            PenstockTunnel | TurbineHall | SwitchgearRoom | SpillwayStair | DrainageGallery => 0.0,
            Balcony | RailYard | CrusherShed => STOREY,
            GateShaft | IntakeGallery | SpillwayGallery | ControlRoom => STOREY,
            WindingHouse | Offices | Headframe => 2.0 * STOREY,
            CrestRoad | TowerHead | SpillwayGatehouse => 2.0 * STOREY,
            SeaCave | Shingle | Strand | Boathouse => 0.0,
            Washhouse | LowerLane | FishMarket | Taverna => STOREY,
            Chapel | Piazza | UpperLane | Pensione => 2.0 * STOREY,
            Lighthouse | LighthouseYard | KeepersCottage | Battery => 3.0 * STOREY,
            TownSquare | WeighHouse | Brewery | CheeseMarket | AlmshouseCourt | Windmill | Warehouse | LockYard | SluiceHouse | TollQuay | Shipyard | CoalYard | HerringQuay
            | CustomsHouse => 0.0,
            DykeRoad | PumpingStation => STOREY,
        }
    }

    /// The Zone's volume. Floors sit a little inside it, so a Player standing on one is inside.
    /// West Hall is two storeys tall because its ramp climbs to the Balcony, and a Zone with a
    /// sunken floor (the Yard's garden, the Tunnel's dip) reaches down into it. A Zone a ramp or
    /// stairs climb out of is tall enough to hold a Player on them, and a Zone with stairs up a
    /// storey (the Dam's) is two storeys tall. A Zone that owns the stairs up to it has a stairwell
    /// in its floor a storey deep, which reaches its volume down to the floor below. Every Map is
    /// centred on the origin, so a Zone's volume is only ever looked up among its own Map's.
    pub fn volume(self) -> (Vec3, Vec3) {
        const LOW: f32 = -0.25;
        const UPPER: f32 = 3.75;
        const TOP: f32 = 8.0;
        const GALLERY: f32 = STOREY - 0.25;
        const CREST: f32 = 2.0 * STOREY - 0.25;
        let (x0, x1, y0, y1, z0, z1) = match self {
            ZoneId::StartRoom => (-14.0, 14.0, LOW, UPPER, 14.0, 26.0),
            ZoneId::WestHall => (-26.0, -14.0, LOW, TOP, -14.0, 26.0),
            ZoneId::Balcony => (-14.0, -10.0, UPPER, TOP, -14.0, 10.0),
            ZoneId::EastHall => (14.0, 26.0, LOW, UPPER, -14.0, 26.0),
            ZoneId::DiningRoom => (-26.0, 0.0, LOW, UPPER, -26.0, -14.0),
            ZoneId::PowerRoom => (0.0, 26.0, LOW, UPPER, -26.0, -14.0),
            ZoneId::Yard => (-14.0, 14.0, LOW, UPPER, -14.0, 14.0),
            ZoneId::Passage => (-13.5, -10.5, LOW, UPPER, -38.0, -26.0),
            ZoneId::Workshop => (-34.0, -4.0, LOW, UPPER, -50.0, -38.0),
            ZoneId::Storeroom => (-4.0, 4.0, LOW, UPPER, -50.0, -38.0),
            ZoneId::GeneratorRoom => (4.0, 34.0, LOW, UPPER, -50.0, -38.0),
            ZoneId::Tunnel => (12.5, 15.5, LOW, UPPER, -38.0, -26.0),
            ZoneId::PumpHouse => (-8.0, 8.0, LOW, UPPER, -30.0, -20.0),
            ZoneId::QuarryFloor => (-20.0, 20.0, LOW, TOP - 0.25, -20.0, 2.0),
            ZoneId::RailYard => (0.0, 20.0, UPPER, 3.0 * STOREY - 0.25, 2.0, 16.0),
            ZoneId::CrusherShed => (-20.0, 0.0, UPPER, TOP - 0.25, 2.0, 16.0),
            ZoneId::WindingHouse => (0.0, 20.0, TOP - 0.25, 3.0 * STOREY - 0.25, 16.0, 28.0),
            ZoneId::Offices => (-20.0, 0.0, TOP - 0.25, 3.0 * STOREY - 0.25, 16.0, 28.0),
            ZoneId::Headframe => (-6.0, 6.0, TOP - 0.25, 4.0 * STOREY, 28.0, 40.0),
            ZoneId::Incline => (-30.0, -20.0, LOW, 3.0 * STOREY - 0.25, -12.0, 30.0),
            ZoneId::LoadingDock => (20.0, 34.0, LOW, UPPER, -20.0, -2.0),
            ZoneId::Inn => (-8.0, 8.0, LOW, UPPER, 6.0, 22.0),
            ZoneId::Market => (-24.0, 24.0, LOW, UPPER, -22.0, 6.0),
            ZoneId::WestEnd => (-72.0, -24.0, LOW, UPPER, -6.0, 6.0),
            ZoneId::EastEnd => (24.0, 72.0, LOW, UPPER, -6.0, 6.0),
            ZoneId::BackLane => (-24.0, 24.0, LOW, UPPER, 22.0, 30.0),
            ZoneId::MillLane => (-72.0, -24.0, LOW, UPPER, 22.0, 30.0),
            ZoneId::ChurchLane => (24.0, 72.0, LOW, UPPER, 22.0, 30.0),
            ZoneId::Mill => (-72.0, -52.0, LOW, UPPER, 6.0, 22.0),
            ZoneId::Smithy => (-40.0, -24.0, LOW, UPPER, 6.0, 22.0),
            ZoneId::Bakery => (-24.0, -8.0, LOW, UPPER, 6.0, 22.0),
            ZoneId::Schoolhouse => (24.0, 40.0, LOW, UPPER, 6.0, 22.0),
            ZoneId::Church => (52.0, 72.0, LOW, UPPER, 6.0, 22.0),
            ZoneId::Orchard => (-72.0, -40.0, LOW, UPPER, 30.0, 54.0),
            ZoneId::Graveyard => (40.0, 72.0, LOW, UPPER, 30.0, 54.0),
            ZoneId::CrestRoad => (-30.0, 30.0, CREST, CREST + STOREY, 22.0, 30.0),
            ZoneId::TowerHead => (-46.0, -30.0, CREST, CREST + STOREY, 18.0, 32.0),
            ZoneId::SpillwayGatehouse => (30.0, 46.0, CREST, CREST + STOREY, 18.0, 32.0),
            ZoneId::GateShaft => (-44.0, -38.0, GALLERY, CREST + STOREY, 2.0, 18.0),
            ZoneId::IntakeGallery => (-44.0, -10.0, GALLERY, CREST, -56.0, 2.0),
            ZoneId::ControlRoom => (-10.0, 10.0, GALLERY, CREST, -6.0, 8.0),
            ZoneId::SpillwayGallery => (10.0, 44.0, GALLERY, CREST + STOREY, -32.0, 18.0),
            ZoneId::PenstockTunnel => (-3.0, 3.0, LOW, CREST, -28.0, -6.0),
            ZoneId::TurbineHall => (-10.0, 10.0, LOW, CREST, -56.0, -28.0),
            ZoneId::SwitchgearRoom => (10.0, 26.0, LOW, UPPER, -52.0, -40.0),
            ZoneId::SpillwayStair => (38.0, 44.0, LOW, CREST, -56.0, -32.0),
            ZoneId::DrainageGallery => (-44.0, 44.0, LOW, CREST, -62.0, -56.0),
            ZoneId::SeaCave => (-56.0, -44.0, LOW, UPPER, -14.0, 6.0),
            ZoneId::Shingle => (-44.0, -22.0, LOW, UPPER, -14.0, 6.0),
            ZoneId::Strand => (-22.0, 20.0, LOW, UPPER, -14.0, 6.0),
            ZoneId::Boathouse => (20.0, 34.0, LOW, UPPER, -14.0, 6.0),
            ZoneId::Washhouse => (-56.0, -44.0, STOREY - 0.25, 2.0 * STOREY - 0.25, 6.0, 24.0),
            ZoneId::LowerLane => (-44.0, -20.0, STOREY - 0.25, 2.0 * STOREY - 0.25, 6.0, 20.0),
            ZoneId::FishMarket => (-20.0, 6.0, STOREY - 0.25, 2.0 * STOREY - 0.25, 6.0, 24.0),
            ZoneId::Taverna => (6.0, 34.0, STOREY - 0.25, 2.0 * STOREY - 0.25, 6.0, 20.0),
            ZoneId::Chapel => (-56.0, -30.0, 2.0 * STOREY - 0.25, 5.0 * STOREY - 0.25, 24.0, 44.0),
            ZoneId::Piazza => (-30.0, -4.0, 2.0 * STOREY - 0.25, 3.0 * STOREY - 0.25, 24.0, 40.0),
            ZoneId::UpperLane => (-4.0, 18.0, 2.0 * STOREY - 0.25, 3.0 * STOREY - 0.25, 24.0, 44.0),
            ZoneId::Pensione => (18.0, 34.0, 2.0 * STOREY - 0.25, 4.0 * STOREY - 0.25, 24.0, 40.0),
            ZoneId::Lighthouse => (-56.0, -40.0, 3.0 * STOREY - 0.25, 6.0 * STOREY - 0.25, 44.0, 64.0),
            ZoneId::LighthouseYard => (-40.0, -12.0, 3.0 * STOREY - 0.25, 4.0 * STOREY - 0.25, 44.0, 64.0),
            ZoneId::KeepersCottage => (-12.0, 6.0, 3.0 * STOREY - 0.25, 4.0 * STOREY - 0.25, 44.0, 64.0),
            ZoneId::Battery => (6.0, 34.0, 3.0 * STOREY - 0.25, 4.0 * STOREY - 0.25, 44.0, 64.0),
            ZoneId::TownSquare => (-26.0, 18.0, LOW, UPPER, -40.0, 0.0),
            ZoneId::WeighHouse => (-56.0, -26.0, LOW, UPPER, -40.0, 0.0),
            ZoneId::Brewery => (-80.0, -56.0, LOW, UPPER, -56.0, 0.0),
            ZoneId::CheeseMarket => (18.0, 54.0, LOW, UPPER, -40.0, 0.0),
            ZoneId::AlmshouseCourt => (-26.0, 40.0, LOW, UPPER, -56.0, -40.0),
            ZoneId::Windmill => (-80.0, -56.0, LOW, TOP, 0.0, 48.0),
            ZoneId::Warehouse => (-56.0, -34.0, LOW, UPPER, 0.0, 24.0),
            ZoneId::LockYard => (-34.0, -13.0, LOW, UPPER, 0.0, 24.0),
            ZoneId::SluiceHouse => (-13.0, 13.0, LOW, UPPER, 0.0, 24.0),
            ZoneId::TollQuay => (13.0, 34.0, LOW, UPPER, 0.0, 24.0),
            ZoneId::Shipyard => (34.0, 58.0, LOW, UPPER, 0.0, 24.0),
            ZoneId::CoalYard => (58.0, 80.0, LOW, UPPER, 0.0, 48.0),
            ZoneId::HerringQuay => (-56.0, 14.0, LOW, UPPER, 24.0, 48.0),
            ZoneId::CustomsHouse => (14.0, 58.0, LOW, UPPER, 24.0, 48.0),
            ZoneId::DykeRoad => (-80.0, 44.0, STOREY - 0.25, 2.0 * STOREY - 0.25, 48.0, 60.0),
            ZoneId::PumpingStation => (44.0, 80.0, STOREY - 0.25, 2.0 * STOREY - 0.25, 48.0, 60.0),
        };
        let sunk = self.map().layout().sunken.iter().filter(|sunken| sunken.zone == self).map(|sunken| sunken.depth).fold(0.0, f32::max);
        (Vec3::new(x0, y0 - sunk, z0), Vec3::new(x1, y1, z1))
    }

    fn bit(self) -> u128 {
        1 << self as u32
    }
}

/// A set of Zones.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ZoneSet(u128);

impl ZoneSet {
    pub fn insert(&mut self, zone: ZoneId) {
        self.0 |= zone.bit();
    }

    /// Every Zone in either set.
    pub fn union(self, other: ZoneSet) -> ZoneSet {
        ZoneSet(self.0 | other.0)
    }

    pub fn contains(self, zone: ZoneId) -> bool {
        self.0 & zone.bit() != 0
    }

    pub fn iter(self) -> impl Iterator<Item = ZoneId> {
        ZoneId::ALL.into_iter().filter(move |&zone| self.contains(zone))
    }
}

impl FromIterator<ZoneId> for ZoneSet {
    fn from_iter<I: IntoIterator<Item = ZoneId>>(iter: I) -> Self {
        let mut set = ZoneSet::default();
        for zone in iter {
            set.insert(zone);
        }
        set
    }
}

/// The Zone the Player is in after moving. They change Zone only once their whole body
/// (`min`..`max`) is inside the new one; until then they stay in the old Zone. Only the Zones of
/// `current`'s own Map are looked at.
pub fn track_zone(current: ZoneId, min: Vec3, max: Vec3) -> ZoneId {
    let fully_inside = |zone: ZoneId| {
        let (lo, hi) = zone.volume();
        min.cmpge(lo).all() && max.cmple(hi).all()
    };
    if fully_inside(current) {
        return current;
    }
    ZoneId::ALL.into_iter().filter(|zone| zone.map() == current.map()).find(|&zone| fully_inside(zone)).unwrap_or(current)
}

/// The Active Zones: the Player's Zone, plus every Zone one open join away from it.
/// `open_joins` lists the joins that are passable now (open Doors, the ramp and the drop).
pub fn active_zones(player: ZoneId, open_joins: impl IntoIterator<Item = (ZoneId, ZoneId)>) -> ZoneSet {
    let mut active = ZoneSet::default();
    active.insert(player);
    for (a, b) in open_joins {
        if a == player {
            active.insert(b);
        } else if b == player {
            active.insert(a);
        }
    }
    active
}

/// The Zone a Player is standing in.
#[derive(Component)]
pub struct PlayerZone(pub ZoneId);

impl PlayerZone {
    /// Where every Player on `map` starts.
    pub fn start(map: MapId) -> Self {
        Self(map.layout().start_zone())
    }
}

/// Zones whose Spawn Points send in Zombies: every Player's Zone and its open neighbours.
#[derive(Resource)]
pub struct ActiveZones(pub ZoneSet);

#[derive(Default, Reflect, GizmoConfigGroup)]
struct ZoneGizmos;

#[derive(Component)]
struct ZoneDebugText;

pub struct ZonePlugin;

impl Plugin for ZonePlugin {
    fn build(&self, app: &mut App) {
        app.init_gizmo_group::<ZoneGizmos>()
            .insert_resource(ActiveZones(ZoneSet::default()))
            .add_systems(OnEnter(GameState::Playing), spawn_debug_text)
            .add_systems(
                Update,
                (
                    (track_player, update_active_zones).chain(),
                    (draw_zones, debug_text).run_if(|view: Res<DebugView>| view.0),
                    show_debug_text,
                )
                    .chain()
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

fn track_player(mut players: Query<(&Transform, &mut PlayerZone), With<Player>>) {
    let half = Vec3::new(PLAYER_RADIUS, PLAYER_RADIUS + PLAYER_CAPSULE_LENGTH / 2.0, PLAYER_RADIUS);
    for (transform, mut zone) in &mut players {
        let at = transform.translation;
        let tracked = track_zone(zone.0, at - half, at + half);
        if tracked != zone.0 {
            zone.0 = tracked;
        }
    }
}

pub(crate) fn update_active_zones(
    players: Query<&PlayerZone, (With<Player>, Without<AwaitingRespawn>)>,
    doors: Query<&Door>,
    pieces: Query<&crate::maps::shifting::PieceState>,
    map: Res<MapId>,
    mut active: ResMut<ActiveZones>,
) {
    let layout = map.layout();
    let rests = crate::maps::shifting::standing_rests(pieces.iter().map(|state| (state.id(), state.phase())), layout.shifting.len());
    let joins: Vec<_> = layout
        .fixed_joins
        .iter()
        .copied()
        .chain(doors.iter().filter(|door| door.open).filter_map(|door| door.joins))
        .chain(crate::maps::shifting::open_joins(layout, &rests, &doors))
        .collect();
    let set = players
        .iter()
        .fold(ZoneSet::default(), |set, zone| set.union(active_zones(zone.0, joins.iter().copied())));
    if set != active.0 {
        active.0 = set;
    }
}

fn draw_zones(mut gizmos: Gizmos<ZoneGizmos>, zone: Single<&PlayerZone, With<LocalPlayer>>, active: Res<ActiveZones>, map: Res<MapId>) {
    for &id in map.layout().zones {
        let (lo, hi) = id.volume();
        let colour = if id == zone.0 {
            Color::WHITE
        } else if active.0.contains(id) {
            Color::srgb(0.2, 1.0, 0.3)
        } else {
            Color::srgb(0.6, 0.2, 0.2)
        };
        let centre = (lo + hi) / 2.0;
        gizmos.cube(Transform::from_translation(centre).with_scale(hi - lo), colour);
    }
}

fn spawn_debug_text(mut commands: Commands) {
    commands.spawn((
        crate::state::InRun,
        ZoneDebugText,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        TextColor(Color::WHITE),
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.5)),
        Node {
            position_type: PositionType::Absolute,
            left: px(16.0),
            top: px(12.0),
            ..default()
        },
        Visibility::Hidden,
    ));
}

fn debug_text(
    zone: Single<&PlayerZone, With<LocalPlayer>>,
    active: Res<ActiveZones>,
    mut text: Single<&mut Text, With<ZoneDebugText>>,
) {
    let names: Vec<_> = active.0.iter().map(ZoneId::name).collect();
    text.0 = format!("Zone: {}\nActive: {}", zone.0.name(), names.join(", "));
}

fn show_debug_text(view: Res<DebugView>, mut visibility: Single<&mut Visibility, With<ZoneDebugText>>) {
    let wanted = if view.0 { Visibility::Inherited } else { Visibility::Hidden };
    visibility.set_if_neq(wanted);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ZoneId::*;

    /// The Greybox's joins that never close.
    const JOINS: &[(ZoneId, ZoneId)] = MapId::Greybox.layout().fixed_joins;

    /// A Player-sized box with its feet at `(x, y, z)`.
    fn body(x: f32, y: f32, z: f32) -> (Vec3, Vec3) {
        (Vec3::new(x - 0.35, y, z - 0.35), Vec3::new(x + 0.35, y + 1.8, z + 0.35))
    }

    fn track(current: ZoneId, feet: (f32, f32, f32)) -> ZoneId {
        let (min, max) = body(feet.0, feet.1, feet.2);
        track_zone(current, min, max)
    }

    #[test]
    fn zone_volumes_of_one_map_do_not_overlap() {
        for a in ZoneId::ALL {
            for b in ZoneId::ALL.into_iter().filter(|&b| b != a && b.map() == a.map()) {
                let ((alo, ahi), (blo, bhi)) = (a.volume(), b.volume());
                let overlap = alo.cmplt(bhi).all() && blo.cmplt(ahi).all();
                assert!(!overlap, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn standing_in_a_room_puts_the_player_in_it() {
        assert_eq!(track(StartRoom, (0.0, 0.0, 20.0)), StartRoom);
        assert_eq!(track(StartRoom, (-20.0, 0.0, 0.0)), WestHall);
        assert_eq!(track(StartRoom, (0.0, 0.0, 0.0)), Yard);
        assert_eq!(track(StartRoom, (-12.0, 4.0, 0.0)), Balcony);
        assert_eq!(track(StartRoom, (-10.0, 0.0, -20.0)), DiningRoom);
        assert_eq!(track(StartRoom, (10.0, 0.0, -20.0)), PowerRoom);
        assert_eq!(track(StartRoom, (20.0, 0.0, 0.0)), EastHall);
    }

    #[test]
    fn a_player_straddling_a_doorway_stays_in_the_old_zone() {
        // The Start–Yard Door is in the wall at z = 14.
        assert_eq!(track(StartRoom, (0.0, 0.0, 14.0)), StartRoom);
        assert_eq!(track(Yard, (0.0, 0.0, 14.0)), Yard);
    }

    #[test]
    fn a_player_changes_zone_once_fully_inside_the_new_one() {
        assert_eq!(track(StartRoom, (0.0, 0.0, 13.7)), StartRoom);
        assert_eq!(track(StartRoom, (0.0, 0.0, 13.6)), Yard);
    }

    #[test]
    fn falling_from_the_balcony_moves_the_player_into_the_yard_only_once_below_it() {
        assert_eq!(track(Balcony, (-11.0, 3.0, 4.0)), Balcony);
        assert_eq!(track(Balcony, (-9.0, 1.0, 4.0)), Yard);
    }

    #[test]
    fn climbing_the_ramp_keeps_the_player_in_west_hall() {
        assert_eq!(track(WestHall, (-15.7, 2.0, 4.0)), WestHall);
        assert_eq!(track(WestHall, (-15.7, 4.0, -2.0)), WestHall);
    }

    #[test]
    fn the_start_room_alone_is_active_with_every_door_shut() {
        let active = active_zones(StartRoom, JOINS.iter().copied());
        assert_eq!(active.iter().collect::<Vec<_>>(), vec![StartRoom]);
    }

    #[test]
    fn open_doors_activate_the_zone_on_the_other_side() {
        let active = active_zones(StartRoom, JOINS.iter().copied().chain([(StartRoom, WestHall)]));
        assert!(active.contains(WestHall));
        assert!(!active.contains(Balcony), "only one join away counts");
    }

    #[test]
    fn west_hall_activates_the_balcony_but_not_the_yard() {
        let active = active_zones(WestHall, JOINS.iter().copied());
        assert!(active.contains(Balcony));
        assert!(!active.contains(Yard));
    }

    #[test]
    fn the_balcony_activates_west_hall_and_the_yard() {
        let active = active_zones(Balcony, JOINS.iter().copied());
        assert_eq!(active, ZoneSet::from_iter([Balcony, WestHall, Yard]));
    }

    #[test]
    fn the_drop_counts_both_ways_for_activation() {
        assert!(active_zones(Yard, JOINS.iter().copied()).contains(Balcony));
    }

    #[test]
    fn a_far_open_door_activates_nothing() {
        let active = active_zones(StartRoom, JOINS.iter().copied().chain([(DiningRoom, PowerRoom)]));
        assert_eq!(active, ZoneSet::from_iter([StartRoom]));
    }
}
